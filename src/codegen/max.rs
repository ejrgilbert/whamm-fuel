use wirm::{Module, Opcode};
use wirm::ir::function::FunctionBuilder;
use wirm::ir::id::LocalID;
use wirm::opcode::Inject;
use wirm::wasmparser::Operator;
use crate::analyze::FuncState;
use crate::codegen::{codegen, handle_reqs, CodeGenResult, CodeGenState};
use crate::run::CompType;
use crate::slice::{Slice, SliceResult};

pub fn codegen_max<'a, 'b>(ty: &CompType, slices: &mut [SliceResult], funcs: &[FuncState], wasm: &Module<'a>, gen_wasm: &mut Module<'b>) -> CodeGenResult where 'a : 'b {
    codegen(ty, slices, CodeGenState::new_max, in_max_slice, gen_op, funcs, wasm, gen_wasm)
}

fn in_max_slice(instr_idx: usize, slice: &Slice) -> bool {
    slice.max_slice.contains(&instr_idx)
}

// Translate instructions into `local.get` on parameter representing that state! (if necessary)
fn gen_op<'a, 'b>(opidx: usize, op: &Operator<'a>, fuel: &LocalID, gen_state: &CodeGenState, func: &mut FunctionBuilder<'b>) where 'a : 'b {
    if handle_reqs(gen_state.for_params.get(&opidx), func) {
    } else if handle_reqs(gen_state.for_globals.get(&opidx), func) {
    } else if handle_reqs(gen_state.for_loads.get(&opidx), func) {
    } else if handle_reqs(gen_state.for_calls.get(&opidx), func) {
    } else if handle_reqs(gen_state.for_call_indirects.get(&opidx), func) {
    } else {
        if let Operator::Return = op {
            func.local_get(*fuel);
        }
        func.inject(op.clone());
    }
    
}
