use wirm::ir::id::{FunctionID, TypeID};
use wirm::ir::module::module_types::Types;
use wirm::iterator::module_iterator::ModuleIterator;
use wirm::{Location, Module};
use wirm::iterator::iterator_trait::Iterator;
use wirm::wasmparser::Operator;
use crate::utils::stack_effects;

/// A provenance/origin for a value
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    /// A constant literal
    Const {
        instr_idx: usize
    },
    /// Unknown / external / untracked
    Untracked
}

/// Lightweight kind of operator we care about for slicing & identification.
#[derive(Debug, Clone)]
pub enum OpKind {
    // i only care about control opcodes!
    Control,      // br_if, if, br_table, br, select (select we treat specially)
    // Load,         // any memory load
    // Store,        // memory store (we don't treat as sources but stack effects matter)
    // GlobalGet,
    // GlobalSet,
    // LocalGet,
    // LocalSet,
    // LocalTee,
    // Const,
    // Binary,
    // Unary,
    // Call,           // simplified: consumes args, produces result (we won't analyze inside)
    // CallIndirect,   // simplified: consumes args, produces result (we won't analyze inside)
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
    stack: Vec<Origin>,             // current stack
    instrs: Vec<InstrInfo>,         // information about instrs (used to create the slice)
}
impl FuncTaint {
    fn new(wasm: &Module, fid: FunctionID) -> FuncTaint {
        let tid = wasm.functions.get(fid).get_type_id();
        let (total_params, total_results) = if let Some(Types::FuncType { params , results, ..}) = wasm.types.get(tid) {
            (params.len(), results.len())
        } else {
            panic!("Should have found a function type!");
        };
        // If I need to compute the total number of locals for a function:
        // let mut num_locals = total_params;
        // let func = wasm.functions.unwrap_local(fid);
        // for (i, _) in func.body.locals.iter() {
        //     num_locals += *i as usize;
        // }

        Self {
            fid: *fid,
            local_origin: vec![],
            total_params,
            total_results,
            ..Default::default()
        }
    }
}

pub fn analyze(wasm: &mut Module) -> Vec<FuncState>{
    let mut mi = ModuleIterator::new(wasm, &vec![]);
    let mut funcs: Vec<FuncState> = Vec::new();

    let mut first = true;
    let mut state = FuncTaint::default();
    while first || mi.next().is_some() {
        let (
            Location::Module {func_idx, instr_idx} |
            Location::Component {func_idx, instr_idx, ..},
            ..
        ) = mi.curr_loc();
        println!("Function #{} at instruction offset: {}", *func_idx, instr_idx);

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

        // TODO -- are all of these handled correctly?
        match op {
            // ---------------- Constants ----------------
            Operator::I32Const { .. }
            | Operator::I64Const { .. }
            | Operator::F32Const { .. }
            | Operator::F64Const { .. } => {
                // push const
                state.stack.push(Origin::Const {instr_idx});
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![]
                });
            }

            // ---------------- Locals ----------------
            Operator::LocalGet { local_index } => {
                // produce whatever the current local maps to (if known), otherwise:
                let origin = state.local_origin
                    .get(*local_index as usize)
                    .cloned()
                    .unwrap_or(
                        if *local_index < state.total_params as u32 {
                            Origin::Param {instr_idx, lid: *local_index}
                        } else {
                            Origin::Untracked
                        }
                    );
                state.stack.push(origin.clone());
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![], // origin already recorded on stack
                });
            }

            Operator::LocalSet { local_index } => {
                // consumes one value and stores into local
                let val = state.stack.pop().unwrap();
                // update local origin
                state.local_origin[*local_index as usize] = val.clone();
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![val],
                });
            }

            Operator::LocalTee { local_index } => {
                // consumes one value, stores into local, and leaves it on stack
                let val = state.stack.pop().unwrap();
                state.local_origin[*local_index as usize] = val.clone();
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

            // ---------------- Stores ----------------
            // Store consumes (addr, value) or (addr,val) depending on op — consume 2.
            Operator::I32Store { .. }
            | Operator::I64Store { .. }
            | Operator::F32Store { .. }
            | Operator::F64Store { .. }
            | Operator::I32Store8 { .. }
            | Operator::I32Store16 { .. }
            | Operator::I64Store8 { .. }
            | Operator::I64Store16 { .. }
            | Operator::I64Store32 { .. } => {
                let val = state.stack.pop().unwrap();
                let addr = state.stack.pop().unwrap();
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![addr, val]
                });
            }

            // ---------------- Binary / Unary ----------------
            Operator::I32Add { .. }
            | Operator::I32Sub { .. }
            | Operator::I32Mul { .. }
            | Operator::I32DivS { .. }
            | Operator::I64Add { .. }
            | Operator::F32Add { .. }
            | Operator::F64Add { .. }
            | Operator::I32Eq { .. }
            | Operator::I32Ne { .. }
            | Operator::I32LtS { .. }
            | Operator::I32GtS { .. }
            | Operator::I32GtU { .. }
            | Operator::I64Eq { .. }
            | Operator::I64Ne { .. }
            | Operator::I64LtS { .. }
            | Operator::I64GtS { .. }
            | Operator::I64GtU { .. }
            => {
                // pop two, push one
                let b = state.stack.pop().unwrap();
                let a = state.stack.pop().unwrap();
                // this instruction produces a new value originating at this instruction idx
                state.stack.push(Origin::Instr {instr_idx});
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![a, b]
                });
            }

            Operator::I32Eqz { .. } | Operator::I32Clz { .. } => {
                let a = state.stack.pop().unwrap();
                state.stack.push(Origin::Instr {instr_idx});
                state.instrs.push(InstrInfo {
                    kind: OpKind::Other,
                    inputs: vec![a]
                });
            }

            // ---------------- Select ----------------
            Operator::Select { .. } => {
                // pops val1, val2, cond  -> pushes result
                let cond = state.stack.pop().unwrap();
                let val2 = state.stack.pop().unwrap();
                let val1 = state.stack.pop().unwrap();
                state.stack.push(Origin::Instr {instr_idx});
                state.instrs.push(InstrInfo {
                    kind: OpKind::Control, // treat select as control-influencing (it's a conditional)
                    inputs: vec![val1, val2, cond]
                });
            }

            // ---------------- Branch / Control ----------------
            Operator::BrIf { .. } | Operator::BrTable { .. }
            | Operator::BrOnNull {..} | Operator::BrOnNonNull {..}
            | Operator::BrOnCast {..} | Operator::BrOnCastFail {..}
            | Operator::If { .. }=> {
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
            }

            // ---------------- Others ----------------
            _ => {
                let (pops, pushes) = stack_effects(op, mi.module);
                let mut inputs = Vec::new();
                for _ in 0..pops {
                    inputs.insert(0, state.stack.pop().unwrap());
                }

                for _ in 0..pushes {
                    state.stack.push(Origin::Untracked)
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