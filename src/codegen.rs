use std::collections::HashMap;
use std::hash::Hash;
use wirm::ir::types::{InitExpr, Value};
use wirm::{DataType, InitInstr, Module, Opcode};
use wirm::ir::function::FunctionBuilder;
use wirm::ir::id::{FunctionID, GlobalID, LocalID};
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

pub struct CodeGenResult {
    pub cost_maps: Vec<HashMap<usize, u64>>,
    pub func_map: HashMap<u32, GeneratedFunc>
}

#[derive(Default)]
pub struct GeneratedFunc {
    pub fid: u32,

    // Maps from dependency index -> generated local ID for each
    // of the types of program state the slice can depend on.
    pub for_params: HashMap<u32, u32>,
    pub for_globals: HashMap<u32, u32>,
    pub for_loads: HashMap<usize, u32>,
    pub for_calls: HashMap<usize, CallState>,
    pub for_call_indirects: HashMap<usize, CallState>,
}
impl GeneratedFunc {
    fn new(fid: u32, state: CodeGen) -> Self {
        Self {
            fid,
            for_params: state.for_params,
            for_globals: state.for_globals,
            for_loads: state.for_loads,
            for_calls: state.for_calls,
            for_call_indirects: state.for_call_indirects,
        }
    }
}

pub struct CallState { pub used_arg: usize, pub gen_param_id: u32 }
#[derive(Default)]
struct CodeGen {
    // Maps from dependency index -> generated local ID for each
    // of the types of program state the slice can depend on.
    for_params: HashMap<u32, u32>,
    for_globals: HashMap<u32, u32>,
    for_loads: HashMap<usize, u32>,
    for_calls: HashMap<usize, CallState>,
    for_call_indirects: HashMap<usize, CallState>,

    // Used to track the current cost of the basic block
    // Once we reach a branching opcode, we need to gen the
    // cost computation before branching!
    // 1. generate computation
    // 2. curr_cost = 0
    curr_cost: u64
}
impl CodeGen {
    fn new(slice: &SliceResult) -> (Self, Vec<DataType>) {
        let mut used_params = Vec::new();

        let for_params = process_needed_state(&slice.params, &mut used_params);
        let for_globals = process_needed_state(&slice.globals, &mut used_params);
        let for_loads = process_needed_state(&slice.loads, &mut used_params);
        let for_calls = process_needed_call(&slice.calls, &mut used_params);
        let for_call_indirects = process_needed_call(&slice.call_indirects, &mut used_params);

        fn process_needed_state<T: Clone + Eq + Hash>(needed_state: &HashMap<T, DataType>, used_params: &mut Vec<DataType>) -> HashMap<T, u32> {
            let mut res = HashMap::default();
            for (s, dt) in needed_state.iter() {
                res.insert(s.clone(), used_params.len() as u32);
                used_params.push(*dt);
            }
            res
        }
        fn process_needed_call(needed_state: &HashMap<(usize, usize), DataType>, used_params: &mut Vec<DataType>) -> HashMap<usize, CallState> {
            let mut res = HashMap::default();
            for ((opidx, arg), dt) in needed_state.iter() {
                res.insert(*opidx, CallState {
                    used_arg: *arg,
                    gen_param_id: used_params.len() as u32
                });
                used_params.push(*dt);
            }
            res
        }

        (Self {
            for_params,
            for_globals,
            for_loads,
            for_calls,
            for_call_indirects,
            curr_cost: 0
        }, used_params)
    }
    // ----- COST
    fn add_cost(&mut self, cost: u64) {
        self.curr_cost += cost;
    }
    fn reset_cost(&mut self) {
        self.curr_cost = 0;
    }
}

pub fn codegen<'a, 'b>(ty: &CompType, slices: &mut [SliceResult], funcs: &[FuncState], wasm: &Module<'a>, gen_wasm: &mut Module<'b>) -> CodeGenResult where 'a : 'b {
    let fuel = gen_wasm.add_global(
        InitExpr::new(vec![InitInstr::Value(Value::I64(INIT_FUEL))]),
        DataType::I64,
        true,
        false
    );
    let mut func_map = HashMap::new();
    // maps from `instr_idx` -> cost of block
    let mut cost_maps = Vec::new();
    for (slice, func) in slices.iter_mut().zip(funcs.iter()) {
        let mut cost_map = HashMap::new();
        let lf = wasm.functions.unwrap_local(FunctionID(func.fid));

        let body = &lf.body.instructions;
        let (mut state, used_params) = CodeGen::new(slice);     // one instance of state per function!

        let mut new_func = FunctionBuilder::new(&used_params, &[]);
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
                // Generate opcode that needs to be placed here in the generated function
                gen_op(i, op, &state, &mut new_func);
            }
        }

        // add the function to the `gen_wasm` and save the fid mapping
        let new_fid = new_func.finish_module(gen_wasm);
        let generated_func = GeneratedFunc::new(*new_fid, state);
        func_map.insert(func.fid, generated_func);

        cost_maps.push(cost_map);
    }

    CodeGenResult {
        cost_maps,
        func_map
    }
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

fn gen_fuel_comp_approx(_fuel: &GlobalID, _state: &mut CodeGen, _func: &mut FunctionBuilder) {
    // TODO
    todo!()
}

// Translate instructions into `local.get` on parameter representing that state! (if necessary)
fn gen_op<'a, 'b>(opidx: usize, op: &Operator<'a>, gen_state: &CodeGen, func: &mut FunctionBuilder<'b>) where 'a : 'b {
    // Handle opcodes that lookup vars by their ID in the original program.
    if let Operator::LocalGet {local_index: id} = op {
        if let Some(new_id) = gen_state.for_params.get(id) {
            func.local_get(LocalID(*new_id));
        }
    } else if let Operator::GlobalGet {global_index: id} = op {
        if let Some(new_id) = gen_state.for_globals.get(id) {
            func.local_get(LocalID(*new_id));
        }
    }

    // Handle opcodes that have relevant program state (memory, calls, etc.)
    else if let Some(new_id) = gen_state.for_loads.get(&opidx) {
        func.local_get(LocalID(*new_id));
    } else if let Some(CallState {gen_param_id, ..}) = gen_state.for_calls.get(&opidx) {
        func.local_get(LocalID(*gen_param_id));
    } else if let Some(CallState {gen_param_id, ..}) = gen_state.for_call_indirects.get(&opidx) {
        func.local_get(LocalID(*gen_param_id));
    } else {
        func.inject(op.clone());
    }
}
