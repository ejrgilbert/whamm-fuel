mod utils;

use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet, VecDeque};
use wirm::iterator::iterator_trait::Iterator;
use wirm::iterator::module_iterator::ModuleIterator;
use wirm::{DataType, InitInstr, Location, Module, Opcode};
use wirm::ir::id::{FunctionID, GlobalID, TypeID};
use wirm::ir::module::module_types::Types;
use wirm::wasmparser::{BlockType, Operator};
use std::io::Write;
use termcolor::{Buffer, BufferWriter, Color, ColorChoice, ColorSpec, WriteColor};
use wirm::ir::function::FunctionBuilder;
use wirm::ir::types::{InitExpr, Instructions, Value};
use wirm::opcode::Inject;
use crate::utils::stack_effects;

const INIT_FUEL: i64 = 1000;
const FUEL_COMPUTATION: CompType = CompType::Exact;
const SPACE_PER_TAB: usize = 4;

/// Conservative static taint-slicing for WebAssembly.
///
/// This program:
///  - Loads a WASM module
///  - For each function, it simulates stack operations conservatively while tracking
///    *taint* (whether a value depends on function params, globals, or memory).
///  - Marks control-flow instructions (if, br_if, br_table, return, call_indirect, call with tainted args, etc.)
///    that use tainted values as *sinks*.
///  - Builds a backward slice (instructions that produced the tainted values).
///
/// Output: annotated listing per function with the instruction offsets that are in the slice.
///
/// Note: This is conservative and intra-procedural by default. Memory is modeled coarsely:
/// once we see a store of a tainted value, we mark memory as tainted globally; loads are considered tainted if memory is tainted.

/// A provenance/origin for a value
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Origin {
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
enum OpKind {
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
struct InstrInfo {
    idx: usize,
    kind: OpKind,
    /// immediate origins used as inputs by this instruction (in order popped)
    inputs: Vec<Origin>
}

/// Result of the slice analysis.
#[derive(Debug, Default)]
struct SliceResult {
    fid: u32,
    total_params: usize,
    /// all instruction indices that are in the backward slice (influencing control).
    instrs: HashSet<usize>,
    /// all instruction indices that are included for support purposes (block structure)
    instrs_support: HashSet<usize>,
    /// function parameter indices that influence control
    params: HashSet<u32>,
    /// global indices (global.get) that influence control
    globals: HashSet<u32>,
    /// load instruction indices that influence control
    loads: HashSet<usize>,
    /// call instruction indices that influence control
    /// AND the actually-used result of that call
    calls: HashSet<(usize, usize)>,
    /// call_indirect instruction indices that influence control
    /// AND the actually-used result of that call
    call_indirects: HashSet<(usize, usize)>,
}

struct FuncState {
    fid: u32,
    total_params: usize,
    instrs: Vec<InstrInfo>,         // information about instrs (used to create the slice)
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

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        bail!("Usage: wasm_taint_slice <file.wasm>");
    }
    let data = std::fs::read(&args[1])?;
    analyze_and_slice(&data)?;
    Ok(())
}

/// Compute backward slice of values that feed control-flow ops inside a function body.
/// - `num_params`: number of parameters (so we can mark `local.get` of param indices as Param).
fn analyze_and_slice(wasm_bytes: &[u8]) -> Result<()> {
    // Read app Wasm into Wirm module
    let mut wasm = Module::parse(&wasm_bytes, false, true).unwrap();

    let func_taints = analyze(&mut wasm);

    // create the slices
    let mut slices = slice(&func_taints);

    // TODO: Calculate costs!

    // generate code for the slices (leave placeholders for the cost calculation)
    let mut gen_wasm = Module::default();
    let code = codegen(&FUEL_COMPUTATION, &mut slices, &func_taints, &wasm, &mut gen_wasm);
    flush_slices(wasm.globals.len(), &slices, &func_taints, &wasm);

    Ok(())
}

fn analyze(wasm: &mut Module) -> Vec<FuncState>{
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
                    idx: instr_idx,
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
                    .unwrap_or_else(|| {
                        if *local_index < state.total_params as u32 {
                            Origin::Param {instr_idx, lid: *local_index}
                        } else {
                            Origin::Untracked
                        }
                    });
                state.stack.push(origin.clone());
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
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
                    idx: instr_idx,
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
                    idx: instr_idx,
                    kind: OpKind::Other,
                    inputs: vec![val]
                });
            }

            // ---------------- Globals ----------------
            Operator::GlobalGet { global_index } => {
                state.stack.push(Origin::Global {instr_idx, gid: *global_index});
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
                    kind: OpKind::Other,
                    inputs: vec![]
                });
            }

            Operator::GlobalSet { .. } => {
                let val = state.stack.pop().unwrap();
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
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
                    idx: instr_idx,
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
                    idx: instr_idx,
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
                    idx: instr_idx,
                    kind: OpKind::Other,
                    inputs: vec![a, b]
                });
            }

            Operator::I32Eqz { .. } | Operator::I32Clz { .. } => {
                let a = state.stack.pop().unwrap();
                state.stack.push(Origin::Instr {instr_idx});
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
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
                    idx: instr_idx,
                    kind: OpKind::Control, // treat select as control-influencing (it's a conditional)
                    inputs: vec![val1, val2, cond]
                });
            }

            // ---------------- Branch / Control ----------------
            Operator::BrIf { .. } | Operator::If { .. } => {
                // pops condition
                let cond = state.stack.pop().unwrap();
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
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
                    idx: instr_idx,
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
                    idx: instr_idx,
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

// ===============
// ==== SLICE ====
// ===============

fn slice(func_taints: &Vec<FuncState>) -> Vec<SliceResult> {
    let mut slices = Vec::new();
    for taint in func_taints.iter() {
        slices.push(slice_func(taint));
    }
    slices
}

fn slice_func(state: &FuncState) -> SliceResult {
    // Start from control instructions' inputs
    let mut worklist: VecDeque<Origin> = VecDeque::new();
    let mut included_instrs: HashSet<usize> = HashSet::new();
    let mut included_params: HashSet<u32> = HashSet::new();
    let mut included_globals: HashSet<u32> = HashSet::new();
    let mut included_loads: HashSet<usize> = HashSet::new();
    let mut included_calls: HashSet<(usize, usize)> = HashSet::new(); // the call_idx AND the result_idx used
    let mut included_call_indirects: HashSet<(usize, usize)> = HashSet::new();

    for (i, info) in state.instrs.iter().enumerate() {
        if let OpKind::Control = info.kind {
            // any input to this control op is a starting point of the backward slice
            for inp in &info.inputs {
                worklist.push_back(inp.clone());
            }
            // and include the control instruction itself
            included_instrs.insert(i);
        }
    }

    // Trace origins backwards
    while let Some(origin) = worklist.pop_front() {
        match origin {
            Origin::Instr {instr_idx} => {
                // if this instruction already included, skip
                if !included_instrs.insert(instr_idx) {
                    continue;
                }
                // push its inputs to the worklist
                for inp in state.instrs.get(instr_idx).map(|i| i.inputs.clone()).unwrap_or_default() {
                    worklist.push_back(inp);
                }
            }

            Origin::Load {instr_idx} => {
                // Mark the load itself as influencing control
                if !included_loads.insert(instr_idx) {
                    continue;
                }
                // I don't care about the load's origins!
                // A load may have inputs (address origins) — include them too
                // if let Some(info) = state.instrs.get(instr_idx) {
                //     for inp in &info.inputs {
                //         worklist.push_back(inp.clone());
                //     }
                // }

                // also include the load instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::Call {instr_idx, result_idx} => {
                // Mark the call itself as influencing control
                if !included_calls.insert((instr_idx, result_idx)) {
                    continue;
                }
                // I don't care about the call's origins!
                // A call may have inputs (address origins) — include them too
                // if let Some(info) = state.instrs.get(instr_idx) {
                //     for inp in &info.inputs {
                //         worklist.push_back(inp.clone());
                //     }
                // }
                // also include the call instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::CallIndirect {instr_idx, result_idx} => {
                // Mark the call itself as influencing control
                if !included_call_indirects.insert((instr_idx, result_idx)) {
                    continue;
                }
                // I don't care about the call's origins!
                // A call may have inputs (address origins) — include them too
                // if let Some(info) = state.instrs.get(instr_idx) {
                //     for inp in &info.inputs {
                //         worklist.push_back(inp.clone());
                //     }
                // }
                // also include the call instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::Global {gid, instr_idx} => {
                included_globals.insert(gid);
                // also include the instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::Param{lid, instr_idx} => {
                included_params.insert(lid);
                // also include the instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::Const{instr_idx} => {
                // also include the instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::Untracked => {}
        }
    }

    SliceResult {
        fid: state.fid,
        total_params: state.total_params,
        instrs: included_instrs,
        params: included_params,
        globals: included_globals,
        loads: included_loads,
        calls: included_calls,
        ..Default::default()
    }
}


// =================
// ==== CODEGEN ====
// =================

enum CompType {
    Exact,
    Approx
}

#[derive(Default)]
struct CodeGen {
    // Maps from dependency index -> generated local ID for each
    // of the types of program state the slice can depend on.
    for_params: HashMap<u32, u32>,
    for_globals: HashMap<u32, u32>,
    for_loads: HashMap<u32, u32>,
    for_calls: HashMap<u32, u32>,

    // Used to track the current cost of the basic block
    // Once we reach a branching opcode, we need to gen the
    // cost computation before branching!
    // 1. generate computation
    // 2. curr_cost = 0
    curr_cost: u64,

    // Block metadata to help determine if we should keep around the structure
    // IF block contains non-block instructions ==> YES
    // When to set these values?
    // ENTER block --> increment block_depth
    // EXIT block --> decrement block_depth; if block_depth == 0? block_has_instrs = false
    // KEEP op --> if block_depth > 0? block_has_instrs = true
    nested_blocks: Vec<usize>, // indices of the blocks we have seen thus far
    block_support_instrs: HashSet<usize>,
    block_has_instrs: bool,
    // whether we need to save the inner-most block for the sake of the slice
    // consider: local.get 0; if {..} else {..}
    // This depends on param0, so we need to save `if` (included in the slice), `else` and `end` (not included in the slice)
    save_block_for_slice: Vec<bool>
}
impl CodeGen {
    // ----- BLOCKS
    fn in_block(&self) -> bool { !self.nested_blocks.is_empty() }
    fn block_enter(&mut self, instr_idx: usize) {
        self.nested_blocks.push(instr_idx);
        self.save_block_for_slice.push(false);
    }
    fn block_exit(&mut self) -> (Option<usize>, Option<bool>) {
        let block_idx = self.nested_blocks.pop();
        let should_save = self.save_block_for_slice.pop();
        if self.nested_blocks.is_empty() {
            self.block_has_instrs = false;
        }
        (block_idx, should_save)
    }
    fn save_block_for_slice(&mut self) {
        let to_save = self.save_block_for_slice.last_mut().unwrap_or_else(|| { unreachable!()});
        *to_save = true;
    }
    fn add_block_support(&mut self, instr_idx: usize) {
        self.block_support_instrs.insert(instr_idx);
    }
    fn use_block_support(&mut self) -> HashSet<usize> {
        let ret = self.block_support_instrs.to_owned();
        self.block_support_instrs.clear();
        ret
    }

    // ----- COST
    fn add_cost(&mut self, cost: u64) {
        self.curr_cost = cost;
    }
    fn reset_cost(&mut self) {
        self.curr_cost = 0;
    }
}

fn codegen<'a, 'b>(ty: &CompType, slices: &mut Vec<SliceResult>, funcs: &Vec<FuncState>, wasm: &Module<'a>, gen_wasm: &mut Module<'b>) -> HashMap<u32, u32> where 'a : 'b {
    let fuel = gen_wasm.add_global(
        InitExpr::new(vec![InitInstr::Value(Value::I64(INIT_FUEL))]),
        DataType::I64,
        true,
        false
    );
    let mut fid_map = HashMap::new();
    for (slice, func) in slices.iter_mut().zip(funcs.iter()) {
        let lf = wasm.functions.unwrap_local(FunctionID(func.fid));
        let Some(Types::FuncType { params , results, ..}) = wasm.types.get(lf.ty_id) else {
            panic!("Should have found a function type!");
        };

        let mut new_func = FunctionBuilder::new(params, results);
        let body = &lf.body.instructions;
        let mut state = CodeGen::default();     // one instance of state per function!

        for (i, op) in body.get_ops().iter().enumerate() {
            let in_slice = slice.instrs.contains(&i);
            let (support_ops, do_fuel_before) = visit_op(op, i, i == body.len() - 1, in_slice, &mut state);
            slice.instrs_support.extend(support_ops);

            if do_fuel_before {
                // Generate the fuel decrement
                gen_fuel_comp(&fuel, &ty, &mut state, &mut new_func);
                state.reset_cost();
            }

            if in_slice {
                // put this opcode in the generated function
                new_func.inject(op.clone());
            }
        }

        // add the function to the `gen_wasm` and save the fid mapping
        let new_fid = new_func.finish_module(gen_wasm);
        fid_map.insert(func.fid, *new_fid);

        // print the codegen state for this function
    }
    fid_map
}

/// Returns: (should_include, do_fuel_before)
/// - support_opcode: whether this opcode should be included in the generated function.
/// - do_fuel_before: whether we should compute the fuel implications at this location
///                 (before emitting this opcode).
fn visit_op(op: &Operator, instr_idx: usize, at_func_end: bool, is_in_slice: bool, state: &mut CodeGen) -> (HashSet<usize>, bool) {
    // compute and increment the cost to calculate for this block
    state.add_cost(op_cost(op));

    let is_block = matches!(op, Operator::If {..} | Operator::Block {..} | Operator::Loop {..});
    let should_include = if is_block {
        // This opcode creates block structure
        state.block_enter(instr_idx);
        if is_in_slice { state.save_block_for_slice(); }
        HashSet::default()
    } else if matches!(op, Operator::End) {
        state.add_block_support(instr_idx);
        let block_has_instrs = state.block_has_instrs;
        let (block_idx, should_save) = if !at_func_end { state.block_exit() } else { (None, None) };
        if block_has_instrs || should_save.unwrap_or_default() {
            let mut res = state.use_block_support();
            if let Some(block_idx) = block_idx {
                res.insert(block_idx);
            }
            // we want to also include all the support ops we've already collected
            res
        } else {
            HashSet::default()
        }
    } else if matches!(op, Operator::Else) {
        state.add_block_support(instr_idx);
        HashSet::default()
    } else {
        if is_in_slice && state.in_block() {
            state.block_has_instrs = true;
        }
        HashSet::default()
    };

    // should only return true for support_opcode if we want to include it, and it's not already in the slice!
    (if !is_in_slice { should_include } else { HashSet::default() }, false)
}

fn op_cost(op: &Operator) -> u64 {
    // TODO: assumes 1 for now
    // match op {
    //
    // }
    1
}

fn gen_fuel_comp(fuel: &GlobalID, ty: &CompType, state: &mut CodeGen, func: &mut FunctionBuilder) {
    match ty {
        CompType::Exact => gen_fuel_comp_exact(fuel, state, func),
        CompType::Approx => gen_fuel_comp_approx(fuel, state, func),
    }
}

fn gen_fuel_comp_exact(fuel: &GlobalID, state: &mut CodeGen, func: &mut FunctionBuilder) {
    func.global_get(*fuel);
    func.i64_const(state.curr_cost as i64);
    func.i64_sub();
    func.global_set(*fuel);
}

fn gen_fuel_comp_approx(fuel: &GlobalID, state: &mut CodeGen, func: &mut FunctionBuilder) {
    // TODO
}

fn flush_slices(num_globals: usize, slices: &Vec<SliceResult>, funcs: &Vec<FuncState>, wasm: &Module) {
    println!("\n================");
    println!("==== SLICES ====");
    println!("================");
    for (slice, func) in slices.iter().zip(funcs.iter()) {
        println!("function #{} ({} instructions):", slice.fid, slice.instrs.len());
        let body = &wasm.functions.unwrap_local(FunctionID(func.fid)).body.instructions;
        let mut tabs = 0;
        print_state_taint(&slice.params, slice.total_params, "params", &mut tabs);
        print_state_taint(&slice.globals, num_globals, "global", &mut tabs);
        print_instr_taint(&slice.loads, "load", &mut tabs);
        print_call_taint(&slice.calls, "calls", &mut tabs);
        print_call_taint(&slice.call_indirects, "call_indirects", &mut tabs);
        fn print_state_taint(taint: &HashSet<u32>, out_of: usize, ty: &str, tabs: &mut i32) {
            *tabs += 1;
            if !taint.is_empty() {
                println!("{}the {ty} taint:", tab(*tabs));
                print!("{}", tab(*tabs));

                for i in 0..out_of {
                    let tainted = taint.contains(&(i as u32));
                    let s = format!("{}{i}, ", if tainted { "*" } else { " " });
                    if tainted {
                        print_tainted(&s);
                    } else {
                        print!("{s}");
                    }
                }
                println!();
            }
            *tabs -= 1;
        }
        fn print_instr_taint(instrs: &HashSet<usize>, ty: &str, tabs: &mut i32) {
            *tabs += 1;
            if !instrs.is_empty() {
                println!("{}the {ty} instrs influencing CF:", tab(*tabs));
                print!("{}", tab(*tabs));

                let mut sorted: Vec<&usize> = instrs.into_iter().collect();
                sorted.sort();
                for instr in sorted.iter() {
                    print_tainted(&format!("*{}, ", **instr));
                }
                println!();
            }
            *tabs -= 1;
        }
        fn print_call_taint(calls: &HashSet<(usize, usize)>, ty: &str, tabs: &mut i32) {
            *tabs += 1;
            if !calls.is_empty() {
                println!("{}the {ty} instrs influencing CF:", tab(*tabs));
                print!("{}", tab(*tabs));

                let mut sorted: Vec<&(usize, usize)> = calls.into_iter().collect();
                sorted.sort();
                for (instr, res) in sorted.iter() {
                    print_tainted(&format!("*(@{}, res{}), ", *instr, *res));
                }
                println!();
            }
            *tabs -= 1;
        }

        tabs += 1;
        println!("{}the function slice:", tab(tabs));
        tabs += 1;
        for (i, instr_info) in func.instrs.iter().enumerate() {
            let in_slice = slice.instrs.contains(&i);
            let in_support = slice.instrs_support.contains(&i);
            let mark = if in_slice { "*" } else if in_support { "~" } else { " " };
            let s = format!("{}{}\t{} {:?}\n", tab(tabs), instr_info.idx, mark, body.get_ops().get(i).unwrap());
            if in_slice {
                print_tainted(&s);
            } else if in_support {
                print_support(&s);
            } else {
                print!("{s}");
            }
        }
        tabs -= 1;
        tabs -= 1;
        println!();
    }
}


// ===========================
// = Terminal Printing Logic =
// ===========================

const WRITE_ERR: &str = "Uh oh, something went wrong while printing to terminal";

fn print_tainted(s: &str) {
    let writer = BufferWriter::stdout(ColorChoice::Always);
    let mut buff = writer.buffer();
    green(true, s, &mut buff);
    writer
        .print(&buff)
        .expect("Uh oh, something went wrong while printing to terminal");
}
fn print_support(s: &str) {
    let writer = BufferWriter::stdout(ColorChoice::Always);
    let mut buff = writer.buffer();
    blue(true, s, &mut buff);
    writer
        .print(&buff)
        .expect("Uh oh, something went wrong while printing to terminal");
}

pub fn color(s: &str, buffer: &mut Buffer, bold: bool, italics: bool, c: Color) {
    buffer
        .set_color(
            ColorSpec::new()
                .set_bold(bold)
                .set_italic(italics)
                .set_fg(Some(c)),
        )
        .expect(WRITE_ERR);
    write!(buffer, "{}", s).expect(WRITE_ERR);
    buffer.set_color(&ColorSpec::default()).expect("TODO: panic message");
}
pub fn blue(bold: bool, s: &str, buffer: &mut Buffer) {
    color(s, buffer, bold, false, Color::Blue)
}
pub fn cyan(bold: bool, s: &str, buffer: &mut Buffer) {
    color(s, buffer, bold, false, Color::Cyan)
}
pub fn green(bold: bool, s: &str, buffer: &mut Buffer) {
    color(s, buffer, bold, false, Color::Green)
}
pub fn magenta(bold: bool, s: &str, buffer: &mut Buffer) {
    color(s, buffer, bold, false, Color::Magenta)
}
pub fn magenta_italics(bold: bool, s: &str, buffer: &mut Buffer) {
    color(s, buffer, bold, true, Color::Magenta)
}
pub fn red(bold: bool, s: &str, buffer: &mut Buffer) {
    color(s, buffer, bold, false, Color::Red)
}
pub fn white(bold: bool, s: &str, buffer: &mut Buffer) {
    color(s, buffer, bold, false, Color::Rgb(193, 193, 193))
}
pub fn grey_italics(bold: bool, s: &str, buffer: &mut Buffer) {
    color(s, buffer, bold, true, Color::White)
}
pub fn yellow(bold: bool, s: &str, buffer: &mut Buffer) {
    color(s, buffer, bold, false, Color::Yellow)
}
pub fn tab(tab: i32) -> String {
    " ".repeat(SPACE_PER_TAB * tab as usize)
}
