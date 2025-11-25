use std::collections::HashSet;
use wirm::ir::function::FunctionBuilder;
use wirm::ir::id::FunctionID;
use wirm::ir::module::module_types::Types;
use wirm::Module;
use wirm::wasmparser::Operator;
use crate::analyze::FuncState;
use crate::slice::SliceResult;

// ===================
// ==== STRUCTURE ====
// ===================
#[derive(Default)]
struct MinSliceState {
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

pub(crate) fn reduce_slice(slices: &mut [SliceResult], funcs: &[FuncState], wasm: &Module) {
    for (result, func) in slices.iter_mut().zip(funcs.iter()) {
        for (_instr_idx, slice) in result.slices.iter_mut() {
            let lf = wasm.functions.unwrap_local(FunctionID(func.fid));

            let body = &lf.body.instructions;
            let mut state = MinSliceState::default();     // one instance of state per function!

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
fn visit_op(op: &Operator, instr_idx: usize, at_func_end: bool, is_in_slice: bool, state: &mut MinSliceState) -> HashSet<usize> {
    todo!()
}