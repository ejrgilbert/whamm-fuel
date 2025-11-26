use wirm::ir::id::{FunctionID, TypeID};
use wirm::ir::module::module_types::Types;
use wirm::iterator::module_iterator::ModuleIterator;
use wirm::{Location, Module};
use wirm::iterator::iterator_trait::Iterator;
use wirm::wasmparser::Operator;
use crate::utils::stack_effects;

#[derive(Debug, Default, Clone)]
pub enum Origin {
    /// Value produced by an instruction index
    Instr {
        instr_idx: usize
    },
    /// Global variable
    Global {
        instr_idx: usize,
        gid: u32,
    },
    /// Function parameter (index)
    Param {
        instr_idx: usize,
        lid: u32,
    },
    /// Memory load at instruction index
    Load {
        instr_idx: usize
    },
    /// Direct call at instruction index
    Call {
        result_idx: usize,
        instr_idx: usize
    },
    /// Indirect call at instruction index
    CallIndirect {
        result_idx: usize,
        instr_idx: usize
    },

    /// Unknown / external / untracked
    #[default]
    Untracked
}

/// Operator we care about for slicing & identification.
#[derive(Debug, Clone)]
pub enum OpKind {
    Control,      // br_if, if, br_table, br, select (select we treat specially)
    Other,
}

/// Record for each instruction we saw.
#[derive(Clone, Debug)]
pub struct InstrInfo {
    pub(crate) kind: OpKind,
    /// immediate origins used as inputs by this instruction (in order popped)
    pub(crate) inputs: Vec<Origin>
}

pub struct FuncState {
    pub(crate) fid: u32,
    pub(crate) total_params: usize,
    pub(crate) instrs: Vec<InstrInfo>,         // information about instrs (used to create the slice)
}
impl FuncState {
    fn new(taint_state: FuncTaint) -> Self {
        Self {
            fid: taint_state.fid,
            total_params: taint_state.total_params,
            instrs: taint_state.instrs
        }
    }
}

#[derive(Default)]
struct FuncTaint {
    fid: u32,
    // current origin of each local (local index -> Origin). Locals include params + locals.
    // At start, parameters are available through local.get (we treat local.get of < num_params as Param).
    local_origin: Vec<Origin>,
    total_params: usize,
    total_results: usize,

    // Some tracking metadata
    // operand stack: each element is an Origin indicating where the value came from.
    stack: Vec<Origin>,                 // current stack
    control_stack: Vec<(usize, usize)>, // (orig_stack_size, num_results): used to remember stack state for nested blocks
    instrs: Vec<InstrInfo>,             // information about instrs (used to create the slice)
}
impl FuncTaint {
    fn new(wasm: &Module, fid: FunctionID) -> FuncTaint {
        // number of locals is total_params + num_locals!
        let lf = wasm.functions.unwrap_local(FunctionID(*fid));
        let Some(Types::FuncType { params: total_params, results: total_results , ..}) = wasm.types.get(lf.ty_id) else {
            panic!("Should have found a function type!");
        };
        let total_locals = total_params.len() + lf.body.num_locals as usize;

        Self {
            fid: *fid,
            local_origin: vec![Origin::default(); total_locals],
            total_params: total_params.len(),
            total_results: total_results.len(),
            ..Default::default()
        }
    }

    fn get_local_origin(&mut self, i: u32, instr_idx: usize) -> Origin {
        if i < self.total_params as u32 {
            Origin::Param {instr_idx, lid: i}
        } else {
            self.local_origin[i as usize].clone()
        }
    }

    fn set_local_origin(&mut self, i: u32, origins: Origin) {
        self.local_origin[i as usize] = origins;
    }

    fn push_control(&mut self, num_results: usize) {
        self.control_stack.push((self.stack.len(), num_results));
    }

    fn pop_control(&mut self) -> (usize, usize) {
        let (orig_stack_height, num_results) = self.control_stack.pop().unwrap();
        let res_stack_height = orig_stack_height + num_results;
        let curr_stack_height = self.stack.len();

        if curr_stack_height < res_stack_height {
            panic!("Something went horribly wrong in the analysis OR your Wasm module is invalid!");
        }

        let num_pops = curr_stack_height - res_stack_height;
        for _ in 0..num_pops {
            self.stack.pop();
        }

        (orig_stack_height, num_results)
    }
}

pub fn analyze(wasm: &mut Module) -> Vec<FuncState> {
    let mut mi = ModuleIterator::new(wasm, &vec![]);
    let mut funcs: Vec<FuncState> = Vec::new();

    let mut first = true;
    let mut state = FuncTaint::default();
    while first || mi.next().is_some() {
        let (
            Location::Module {func_idx, instr_idx} |
            Location::Component {func_idx, instr_idx, ..},
            is_func_end
        ) = mi.curr_loc();

        if instr_idx == 0 {
            // we're at the start of a new function! --> reset state
            if !first {
                // only save if this isn't the first function we're visiting
                assert!(state.stack.len() == state.total_results || state.stack.is_empty(), "still had stack values leftover: {:?}", state.stack);
                funcs.push(FuncState::new(state));
            }

            state = FuncTaint::new(mi.module, func_idx);
            first = false;
        }

        let op = mi.curr_op().unwrap_or_else(|| {
            panic!("Unable to get current operator");
        });

        match op {
            // ---------------- Locals ----------------
            Operator::LocalGet { local_index } => {
                // produce whatever the current local maps to (if known), otherwise:
                let origin = state.get_local_origin(*local_index, instr_idx);
                state.stack.push(origin.clone());
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![], // origin already recorded on stack
                });
            }

            Operator::LocalSet { local_index } => {
                // consumes one value and stores into local
                let val = state.stack.pop().unwrap();
                state.set_local_origin(*local_index, val.clone());
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![val],
                });
            }

            Operator::LocalTee { local_index } => {
                // consumes one value, stores into local, and leaves it on stack
                let val = state.stack.pop().unwrap();
                state.set_local_origin(*local_index, val.clone());
                // push same origin back
                state.stack.push(val.clone());
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![val]
                });
            }

            // ---------------- Globals ----------------
            Operator::GlobalGet { global_index } => {
                state.stack.push(Origin::Global {instr_idx, gid: *global_index});
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![]
                });
            }

            Operator::GlobalSet { .. } => {
                let val = state.stack.pop().unwrap();
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![val]
                });
            }

            // ---------------- Loads ----------------
            // All loads consume an address (i32) and produce a value.
            Operator::I32Load { .. }
            | Operator::I64Load { .. }
            | Operator::F32Load { .. }
            | Operator::F64Load { .. }
            | Operator::I32Load8S { .. }
            | Operator::I32Load8U { .. }
            | Operator::I32Load16S { .. }
            | Operator::I32Load16U { .. }
            | Operator::I64Load8S { .. }
            | Operator::I64Load8U { .. }
            | Operator::I64Load16S { .. }
            | Operator::I64Load16U { .. }
            | Operator::I64Load32S { .. }
            | Operator::I64Load32U { .. } => {
                let addr_origin = state.stack.pop().unwrap();
                // mark produced value as coming from this load instruction (instr_idx)
                state.stack.push(Origin::Load {instr_idx});
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![addr_origin]
                });
            }

            // ---------------- Branch / Control ----------------
            Operator::BrIf { .. } | Operator::BrTable { .. }
            | Operator::BrOnNull {..} | Operator::BrOnNonNull {..}
            | Operator::BrOnCast {..} | Operator::BrOnCastFail {..} => {
                // pops condition
                let cond = state.stack.pop().unwrap();
                state.instrs.push(InstrInfo {
                    kind: OpKind::Control,
                    inputs: vec![cond]
                });
            }

            // ---------------- Calls ----------------
            Operator::Call {..} | Operator::CallIndirect {..} => {
                let (tid, kind) = if let Operator::Call { function_index } = op {
                    (mi.module.functions.get(FunctionID(*function_index)).get_type_id(), OpKind::Other)
                } else if let Operator::CallIndirect {type_index, ..} = op {
                    (TypeID(*type_index), OpKind::Other)
                } else {
                    unreachable!()
                };
                let (pops, pushes) = if let Some(Types::FuncType { params , results, ..}) = mi.module.types.get(tid) {
                    (params.len(), results.len())
                } else {
                    panic!("Should have found a function type!");
                };
                // conservative: assume 1 arg popped and 1 result produced (not precise)
                // ideally, use type information to know the real parameter count and results
                let mut inputs = Vec::new();
                for _ in 0..pops {
                    inputs.insert(0, state.stack.pop().unwrap());
                }

                for i in 0..pushes {
                    state.stack.push(if let Operator::Call { .. } = op {
                        Origin::Call {
                            result_idx: i,
                            instr_idx
                        }
                    } else if let Operator::CallIndirect {..} = op {
                        Origin::CallIndirect {
                            result_idx: i,
                            instr_idx
                        }
                    } else {
                        unreachable!()
                    })
                }
                state.instrs.push(InstrInfo {
                    kind,
                    inputs
                });
            }

            Operator::Return {..} => {
                for _ in 0..state.total_results {
                    state.stack.pop();
                }
                state.instrs.push(InstrInfo {
                    kind: OpKind::Control,
                    inputs: vec![]
                });
            }

            Operator::If { .. } | Operator::Block { .. } | Operator::Loop { .. } => {
                let (inputs, kind) = if matches!(op, Operator::If { .. }) {
                    // pops condition
                    let cond = state.stack.pop().unwrap();
                    (vec![cond], OpKind::Control)
                } else {
                    (vec![], OpKind::Other)
                };
                let (_, num_results) = stack_effects(op, mi.module);
                state.push_control(num_results);
                state.instrs.push(InstrInfo {
                    kind,
                    inputs
                });
            }

            Operator::End => {
                // We reach an end if we're exiting a control block!
                // need to pop the appropriate values off the stack
                if !is_func_end {
                    state.pop_control();
                }
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![]
                });
            },

            // ---------------- Others ----------------
            _ => {
                let (pops, pushes) = stack_effects(op, mi.module);
                let mut inputs = Vec::new();
                for i in 0..pops {
                    inputs.insert(0, state.stack.pop().unwrap_or_else( || {
                        unreachable!("Issue when popping @{} for opcode: {op:?}", i)
                    }));
                }

                for _ in 0..pushes {
                    state.stack.push(Origin::Instr {instr_idx})
                }
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs
                });
            }
        }
    }
    // push the state of the final function
    assert!(state.stack.len() == state.total_results || state.stack.is_empty(), "still had stack values leftover: {:?}", state.stack);
    funcs.push(FuncState::new(state));

    funcs
}