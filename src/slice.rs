use std::collections::{HashMap, HashSet, VecDeque};
use wirm::ir::id::{FunctionID, GlobalID, TypeID};
use wirm::ir::module::module_types::Types;
use wirm::{DataType, Module};
use wirm::ir::module::module_globals::{GlobalKind, ImportedGlobal, LocalGlobal};
use wirm::wasmparser::Operator;
use crate::analyze::{FuncState, InstrInfo, OpKind, Origin};
use crate::utils::{find_subsection_end, is_branching_op, is_loop};

/// Result of the slice analysis.
#[derive(Debug, Default)]
pub struct SliceResult {
    pub(crate) fid: u32,
    pub(crate) total_params: usize,
    /// Maps from instr_idx -> Slice
    /// There can be 1->many slices for a function
    /// due to how we're handling `loop` blocks!
    pub(crate) slices: HashMap<usize, Slice>,
}
impl SliceResult {
    fn new(fid: u32, total_params: usize) -> Self {
        Self {
            fid, total_params, ..Default::default()
        }
    }
    fn add_slice(&mut self, instr_idx: usize, slice: Slice) {
        self.slices.insert(instr_idx, slice);
    }
}
#[derive(Debug, Default)]
pub struct Slice {
    pub(crate) start_instr_idx: usize,  // (inclusive)
    pub(crate) end_instr_idx: usize,    // (exclusive)
    pub(crate) spec_name: String,
    /// all instruction indices that are in the MAXIMAL backward slice (influencing control).
    pub(crate) max_slice: HashSet<usize>,
    /// all instruction indices that are in the MINIMAL backward slice (influencing control).
    pub(crate) min_slice: HashSet<usize>,
    /// all instruction indices that are included for support purposes (block structure)
    pub(crate) instrs_support: HashSet<usize>,
    /// local.get instruction indices that tie back to a
    /// function parameter that influence control
    /// remembers the parameter type as well.
    pub(crate) params: HashMap<(u32, usize), DataType>,         // (local_id, instr_idx) -> datatype
    /// global.get instruction indices that influence control
    /// remembers the parameter type as well.
    pub(crate) globals: HashMap<(u32, usize), DataType>,        // (local_id, instr_idx) -> datatype
    /// load instruction indices that influence control
    /// remembers the value's type as well.
    pub(crate) loads: HashMap<usize, DataType>,
    /// call instruction indices that influence control
    /// AND the actually-used result of that call
    /// remembers the value's type as well.
    pub(crate) calls: HashMap<(usize, usize), DataType>,
    /// call_indirect instruction indices that influence control
    /// AND the actually-used result of that call
    /// remembers the value's type as well.
    pub(crate) call_indirects: HashMap<(usize, usize), DataType>,

    /// This is for the minimum slice, stores the needed `taken` state
    pub(crate) taken: HashMap<usize, DataType>,
}

pub fn slice_program(func_taints: &[FuncState], wasm: &Module) -> Vec<SliceResult> {
    let mut results = Vec::new();
    for taint in func_taints.iter() {
        let lf = wasm.functions.unwrap_local(FunctionID(taint.fid));
        let Some(Types::FuncType { params , ..}) = wasm.types.get(lf.ty_id) else {
            panic!("Should have found a function type!");
        };
        let mut result = SliceResult::new(taint.fid, taint.total_params);
        slice(&mut result, taint.fid, "".to_string(), 0, &taint.instrs, params, wasm);
        results.push(result);
    }
    results
}

fn slice(result: &mut SliceResult, fid: u32, spec_name: String, true_start: usize, instrs_info: &[InstrInfo], func_params: &[DataType], wasm: &Module) {
    let op_at = |instr_idx: usize| -> &Operator {
        let lf = wasm.functions.unwrap_local(FunctionID(fid));
        lf.body.instructions.get_ops().get(instr_idx).unwrap()
    };
    // Start from control instructions' inputs
    let mut worklist: VecDeque<Origin> = VecDeque::new();
    let mut included_instrs: HashSet<usize> = HashSet::new();
    // TODO -- track this as included instruction results! Not as the value at the end of a function!
    let mut included_params: HashMap<(u32, usize), DataType> = HashMap::new();
    let mut included_globals: HashMap<(u32, usize), DataType> = HashMap::new();
    let mut included_loads: HashMap<usize, DataType> = HashMap::new();
    let mut included_calls: HashMap<(usize, usize), DataType> = HashMap::new(); // the call_idx AND the result_idx used
    let mut included_call_indirects: HashMap<(usize, usize), DataType> = HashMap::new();

    let mut i = 0;
    while i < instrs_info.len() {
        let true_instr_idx = true_start + i;
        let info = &instrs_info[i];

        if is_loop(true_instr_idx, op_at(true_instr_idx)).is_some() {
            let lf = wasm.functions.unwrap_local(FunctionID(fid));
            let body = lf.body.instructions.get_ops();
            let end = find_subsection_end(&body[i+1..]); // exclusive end index within body[i+1..]
            let sub_sec = &instrs_info[i+1..i+1+end];

            // Recurse on the subsection
            let spec_name = format!("_loop_at_{true_instr_idx}");
            slice(result, fid, spec_name, true_instr_idx + 1, sub_sec, func_params, wasm);

            // Move i past the subsection so we don't reprocess it (skip special opcode and its END)
            i += end + 1;
        } else if let OpKind::Control = info.kind {
            // any input to this control op is a starting point of the backward slice
            for inp in &info.inputs {
                worklist.push_back(inp.clone());
            }
            // and include the control instruction itself
            included_instrs.insert(true_instr_idx);
        }
        i += 1;
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
                for inp in instrs_info.get(instr_idx).map(|i| i.inputs.clone()).unwrap_or_default() {
                    worklist.push_back(inp);
                }
            }

            Origin::Load {instr_idx} => {
                let load_ty = match op_at(instr_idx) {
                    Operator::I32Load { .. }
                    | Operator::I32Load8S { .. }
                    | Operator::I32Load8U { .. }
                    | Operator::I32Load16S { .. }
                    | Operator::I32Load16U { .. } => DataType::I32,
                    Operator::I64Load { .. }
                    | Operator::I64Load8S { .. }
                    | Operator::I64Load8U { .. }
                    | Operator::I64Load16S { .. }
                    | Operator::I64Load16U { .. }
                    | Operator::I64Load32S { .. }
                    | Operator::I64Load32U { .. } => DataType::I64,
                    Operator::F32Load { .. } => DataType::F32,
                    Operator::F64Load { .. } => DataType::F64,
                    op => panic!("Load opcode not supported: {op:?}")
                };

                // Mark the load itself as influencing control
                if included_loads.insert(instr_idx, load_ty).is_some() {
                    continue;
                }

                // also include the load instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::Call {instr_idx, result_idx} => {
                let call_arg_ty = match op_at(instr_idx) {
                    Operator::Call { function_index } => {
                        let Some(Types::FuncType { results, ..}) = wasm.types.get(wasm.functions.get_type_id(FunctionID(*function_index))) else {
                            panic!("Should have found a function type!");
                        };
                        *results.get(result_idx).unwrap()
                    },
                    op => panic!("Call opcode not supported: {op:?}")
                };

                // Mark the call itself as influencing control
                if included_calls.insert((instr_idx, result_idx), call_arg_ty).is_some() {
                    continue;
                }
                // also include the call instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::CallIndirect {instr_idx, result_idx} => {
                let call_arg_ty = match op_at(instr_idx) {
                    Operator::CallIndirect { type_index, .. } => {
                        let Some(Types::FuncType { results, ..}) = wasm.types.get(TypeID(*type_index)) else {
                            panic!("Should have found a function type!");
                        };
                        *results.get(result_idx).unwrap()
                    },
                    op => panic!("CallIndirect opcode not supported: {op:?}")
                };

                // Mark the call itself as influencing control
                if included_call_indirects.insert((instr_idx, result_idx), call_arg_ty).is_some() {
                    continue;
                }
                // also include the call instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::Global {gid, instr_idx} => {
                let kind = wasm.globals.get_kind(GlobalID(gid));
                let (GlobalKind::Local(LocalGlobal {ty, ..}) |
                GlobalKind::Import(ImportedGlobal {ty, ..})) = kind;
                let global_ty = DataType::from(ty.content_type);

                included_globals.insert((gid, instr_idx), global_ty);
                // also include the instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::Param{lid, instr_idx} => {
                let param_ty = *func_params.get(lid as usize).unwrap();
                included_params.insert((lid, instr_idx), param_ty);
                // also include the instruction index in the instr set
                included_instrs.insert(instr_idx);
            }

            Origin::Untracked => {}
        }
    }

    result.add_slice(
        true_start,
        Slice {
            start_instr_idx: true_start,
            end_instr_idx: true_start + instrs_info.len(),
            spec_name,
            max_slice: included_instrs,
            params: included_params,
            globals: included_globals,
            loads: included_loads,
            calls: included_calls,
            call_indirects: included_call_indirects,
            ..Default::default()
        }
    );
}

// ===================
// ==== STRUCTURE ====
// ===================
#[derive(Default)]
struct IdentifyStructure {
    // Block metadata to help determine if we should keep around the structure
    // IF block contains non-block instructions ==> YES
    // When to set these values?
    // ENTER block --> increment block_depth
    // EXIT block --> decrement block_depth; if block_depth == 0? block_has_instrs = false
    // KEEP op --> if block_depth > 0? block_has_instrs = true
    nested_blocks: Vec<usize>, // indices of the blocks we have seen thus far
    block_support_instrs: HashSet<usize>,
    block_has_instrs: bool,
    // whether we need to save the innermost block for the sake of the slice
    // consider: local.get 0; if {..} else {..}
    // This depends on param0, so we need to save `if` (included in the slice), `else` and `end` (not included in the slice)
    save_block_for_slice: Vec<bool>
}
impl IdentifyStructure {
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
}

pub fn save_structure(slices: &mut [SliceResult], funcs: &[FuncState], wasm: &Module) {
    for (result, func) in slices.iter_mut().zip(funcs.iter()) {
        for (_instr_idx, slice) in result.slices.iter_mut() {
            let lf = wasm.functions.unwrap_local(FunctionID(func.fid));

            let body = &lf.body.instructions;
            let mut state = IdentifyStructure::default();     // one instance of state per function!

            for (i, op) in body.get_ops().iter().enumerate() {
                let in_slice = slice.max_slice.contains(&i);
                let support_ops = visit_op(op, i, i == body.len() - 1, in_slice, &mut state);
                slice.instrs_support.extend(support_ops);
            }
        }
    }
}

/// Returns: (should_include, do_fuel_before)
/// - support_opcode: whether this opcode should be included in the generated function.
/// - do_fuel_before: whether we should compute the fuel implications at this location
///   (before emitting this opcode).
fn visit_op(op: &Operator, instr_idx: usize, at_func_end: bool, is_in_slice: bool, state: &mut IdentifyStructure) -> HashSet<usize> {
    // Test whether we need to save extra support opcodes
    let is_cf = is_branching_op(op) || matches!(op,
        // control opcodes
        Operator::Return
    );
    let is_block = matches!(op, Operator::If {..} | Operator::Block {..} | Operator::Loop {..});
    let should_include = if is_block {
        // This opcode creates block structure
        state.block_enter(instr_idx);
        if is_in_slice { state.save_block_for_slice(); }
        HashSet::default()
    } else if matches!(op, Operator::Else) {
        state.add_block_support(instr_idx);
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
    } else {
        if is_in_slice && state.in_block() {
            state.block_has_instrs = true;
        }
        if is_cf {
            // this is some extra control flow that we'll want to
            // include if we have an included slice in this block
            state.add_block_support(instr_idx);
        }
        HashSet::default()
    };

    // should only return true for support_opcode if we want to include it, and it's not already in the slice!
    if !is_in_slice { should_include } else { HashSet::default() }
}
