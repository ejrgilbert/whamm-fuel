use wirm::ir::id::FunctionID;
use wirm::{DataType, Module};
use wirm::wasmparser::Operator;
use crate::analyze::FuncState;
use crate::slice::SliceResult;
use crate::utils::is_branching_op;

pub(crate) fn reduce_slice(slices: &mut [SliceResult], funcs: &[FuncState], wasm: &Module) {
    for (result, func) in slices.iter_mut().zip(funcs.iter()) {
        for (_instr_idx, slice) in result.slices.iter_mut() {
            let lf = wasm.functions.unwrap_local(FunctionID(func.fid));
            let body = &lf.body.instructions;

            for (i, op) in body.get_ops().iter().enumerate() {
                let in_support = slice.instrs_support.contains(&i);
                let (in_min_slice, need_taken) = visit_op(op);
                if in_min_slice && !in_support {
                    slice.min_slice.insert(i);
                }
                if let Some(dt) = need_taken {
                    slice.taken.insert(i, dt);
                }
            }
        }
    }
}

/// Returns: (should_include, do_fuel_before)
/// - support_opcode: whether this opcode should be included in the generated function.
/// - do_fuel_before: whether we should compute the fuel implications at this location
///   (before emitting this opcode).
/// Returns (in_min_slice, need_taken)
fn visit_op(op: &Operator) -> (bool, Option<DataType>) {
    // If this opcode is in the slice && it's a branching opcode, I want to know if the branch was taken
    let in_min_slice = is_branching_op(op) || matches!(op, Operator::If {..} | Operator::Return);
    let need_taken = if in_min_slice && is_branching_op(op) && !matches!(op, Operator::Br {..}) || matches!(op, Operator::If {..}) {
        Some(DataType::I32)
    } else {
        None
    };

    (in_min_slice, need_taken)
}
