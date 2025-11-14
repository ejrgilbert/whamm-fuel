use std::collections::HashMap;
use wirm::ir::types::{InitExpr, Value};
use wirm::{DataType, InitInstr, Module, Opcode};
use wirm::ir::function::FunctionBuilder;
use wirm::ir::id::{FunctionID, GlobalID};
use wirm::ir::module::module_types::Types;
use wirm::opcode::Inject;
use wirm::wasmparser::Operator;
use crate::analyze::FuncState;
use crate::INIT_FUEL;
use crate::slice::SliceResult;
use crate::utils::is_branching_op;

pub enum CompType {
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
    curr_cost: u64
}
impl CodeGen {
    // ----- COST
    fn add_cost(&mut self, cost: u64) {
        self.curr_cost += cost;
    }
    fn reset_cost(&mut self) {
        self.curr_cost = 0;
    }
}

pub fn codegen<'a, 'b>(ty: &CompType, slices: &mut [SliceResult], funcs: &[FuncState], wasm: &Module<'a>, gen_wasm: &mut Module<'b>) -> (Vec<HashMap<usize, u64>>, HashMap<u32, u32>) where 'a : 'b {
    let fuel = gen_wasm.add_global(
        InitExpr::new(vec![InitInstr::Value(Value::I64(INIT_FUEL))]),
        DataType::I64,
        true,
        false
    );
    let mut fid_map = HashMap::new();
    // maps from `instr_idx` -> cost of block
    let mut cost_maps = Vec::new();
    for (slice, func) in slices.iter_mut().zip(funcs.iter()) {
        let mut cost_map = HashMap::new();
        let lf = wasm.functions.unwrap_local(FunctionID(func.fid));
        let Some(Types::FuncType { params , results, ..}) = wasm.types.get(lf.ty_id) else {
            panic!("Should have found a function type!");
        };

        // TODO -- the results == 0; the params == the state we need!
        let mut new_func = FunctionBuilder::new(params, results);
        let body = &lf.body.instructions;
        let mut state = CodeGen::default();     // one instance of state per function!

        for (i, op) in body.get_ops().iter().enumerate() {
            let in_slice = slice.instrs.contains(&i);
            let in_support = slice.instrs_support.contains(&i);
            let do_fuel_before = calc_op_cost(in_slice | in_support, i == body.len() - 1, op, &mut state);

            if do_fuel_before {
                // Generate the fuel decrement
                let cost = state.curr_cost;
                gen_fuel_comp(&fuel, ty, &mut state, &mut new_func);
                state.reset_cost();
                cost_map.insert(i, cost);
            }

            if in_slice | in_support {
                // put this opcode in the generated function
                new_func.inject(op.clone());
            }
        }

        // add the function to the `gen_wasm` and save the fid mapping
        let new_fid = new_func.finish_module(gen_wasm);
        fid_map.insert(func.fid, *new_fid);

        cost_maps.push(cost_map);

        // print the codegen state for this function
    }
    (cost_maps, fid_map)
}

/// Returns: (should_include, do_fuel_before)
/// - support_opcode: whether this opcode should be included in the generated function.
/// - do_fuel_before: whether we should compute the fuel implications at this location
///   (before emitting this opcode).
fn calc_op_cost(is_in_slice: bool, at_func_end: bool, op: &Operator, state: &mut CodeGen) -> bool {
    // compute and increment the cost to calculate for this block
    state.add_cost(op_cost(op));

    let is_cf = is_branching_op(op) || matches!(op,
        // block
        Operator::Else | Operator::End |
        // control opcodes
        Operator::Return
    );

    if (is_cf && is_in_slice) || at_func_end {
        // If we're at a control flow opcode in the computed slice OR
        // we're at the end of the function -> we need to insert logic that
        // decrements the fuel (right before this instr)
        true
    } else {
        false
    }
}

fn op_cost(_op: &Operator) -> u64 {
    // TODO: assumes 1 for now
    1
}

fn gen_fuel_comp(fuel: &GlobalID, ty: &CompType, state: &mut CodeGen, func: &mut FunctionBuilder) {
    match ty {
        CompType::Exact => gen_fuel_comp_exact(fuel, state, func),
        CompType::Approx => gen_fuel_comp_approx(fuel, state, func),
    }
}

fn gen_fuel_comp_exact(fuel: &GlobalID, state: &mut CodeGen, func: &mut FunctionBuilder) {
    if state.curr_cost > 0 {
        func.global_get(*fuel);
        func.i64_const(state.curr_cost as i64);
        func.i64_sub();
        func.global_set(*fuel);
    }
}

fn gen_fuel_comp_approx(fuel: &GlobalID, state: &mut CodeGen, func: &mut FunctionBuilder) {
    // TODO
    todo!()
}
