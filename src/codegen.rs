use std::collections::HashMap;
use std::hash::Hash;
use wirm::ir::types::{BlockType, InitExpr, Value};
use wirm::{DataType, InitInstr, Module, Opcode};
use wirm::ir::function::FunctionBuilder;
use wirm::ir::id::{FunctionID, GlobalID, LocalID};
use wirm::module_builder::AddLocal;
use wirm::opcode::Inject;
use wirm::wasmparser::Operator;
use crate::analyze::FuncState;
use crate::run::{CompType, FUEL_EXPORT, INIT_FUEL};
use crate::slice::{Slice, SliceResult};
use crate::utils::is_branching_op;

pub struct CodeGenResult {
    /// The instr_idx and the cost calculation to insert at that location!
    pub cost_maps: Vec<HashMap<usize, u64>>,
    /// We can generate 1->many functions per original function
    pub func_map: HashMap<u32, Vec<GeneratedFunc>>
}

#[derive(Default)]
pub struct GeneratedFunc {
    pub fid: u32,
    pub fname: String,

    // Maps from dependency index -> generated local ID for each
    // of the types of program state the slice can depend on.
    pub for_params: HashMap<u32, u32>,
    pub for_globals: HashMap<u32, u32>,
    pub for_loads: HashMap<usize, u32>,
    pub for_calls: HashMap<usize, CallState>,
    pub for_call_indirects: HashMap<usize, CallState>,
}
impl GeneratedFunc {
    fn new(fid: u32, fname: String, state: CodeGen) -> Self {
        Self {
            fid,
            fname,
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
    fn new(slice: &Slice) -> (Self, Vec<DataType>) {
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
    let mut func_map = HashMap::new();
    // maps from `instr_idx` -> cost of block
    let mut cost_maps = Vec::new();
    for (func_slices, func) in slices.iter_mut().zip(funcs.iter()) {
        // We're going to have one instance of cost_map per function because it's tied to the
        // ORIGINAL function, not the generated functions (there can be many per original function
        // due to how we handle `loop` blocks.
        let mut cost_map = HashMap::new();
        let lf = wasm.functions.unwrap_local(FunctionID(func.fid));

        let body = &lf.body.instructions;

        let generated_funcs = gen_from_slices(func.fid, body.get_ops(), func_slices, &mut cost_map, ty, gen_wasm);
        func_map.insert(func.fid, generated_funcs);

        cost_maps.push(cost_map);
    }

    CodeGenResult {
        cost_maps,
        func_map
    }
}

fn gen_from_slices<'a, 'b>(orig_fid: u32, body: &[Operator<'a>], func_slices: &SliceResult, cost_map: &mut HashMap<usize, u64>, ty: &CompType, gen_wasm: &mut Module<'b>) -> Vec<GeneratedFunc> where 'a: 'b {
    let mut generated_funcs = vec![];

    let mut i = 0;
    while i < body.len() {
        if let Some(slice) = func_slices.slices.get(&i) {
            // I know I need to generate a function for this slice!
            let subsec = &body[slice.start_instr_idx..slice.end_instr_idx];
            gen_func(slice.start_instr_idx, &slice.spec_name, cost_map, orig_fid, subsec, slice, func_slices, ty, gen_wasm, &mut generated_funcs);
        }
        i += 1;
    }

    generated_funcs
}

fn gen_func<'a, 'b>(true_start_idx: usize, spec_name: &str, cost_map: &mut HashMap<usize, u64>, orig_fid: u32, body: &[Operator<'a>], slice: &Slice, func_slices: &SliceResult, ty: &CompType, gen_wasm: &mut Module<'b>, generated_funcs: &mut Vec<GeneratedFunc>) where 'a: 'b {
    let (mut state, used_params) = CodeGen::new(slice);     // one instance of state per function!
    let fuel_ty = DataType::I64;
    let mut new_func = FunctionBuilder::new(&used_params, &[fuel_ty.clone()]);
    let fuel = new_func.add_local(fuel_ty.clone());

    // Wrap the function with a block/end to simplify handling of branching from a function
    // (through br depth rather than return opcode)
    // new_func.block(BlockType::Type(fuel_ty));
    new_func.block(BlockType::Empty);

    let mut i = 0;
    while i < body.len() {
        let mut true_instr_idx = true_start_idx + i;
        if true_instr_idx != slice.start_instr_idx {
            if let Some(subslice) = func_slices.slices.get(&true_instr_idx) {
                // if there's a subslice here, skip over its instructions
                i = subslice.end_instr_idx + 1;
                true_instr_idx = true_start_idx + i;
            }
        }

        let op = &body[i];

        let in_slice = slice.instrs.contains(&true_instr_idx);
        let in_support = slice.instrs_support.contains(&true_instr_idx);
        let do_fuel_before = calc_op_cost(in_slice | in_support, i == body.len() - 1, op, &mut state);

        if do_fuel_before {
            // Generate the fuel decrement
            let cost = state.curr_cost;
            gen_fuel_comp(&fuel, ty, &mut state, &mut new_func);
            state.reset_cost();
            cost_map.insert(true_instr_idx, cost);
        }

        if in_slice | in_support {
            // Generate opcode that needs to be placed here in the generated function
            gen_op(true_instr_idx, op, &fuel, &state, &mut new_func);
        }
        i += 1;
    }
    // END the added, wrapping block (see above)
    new_func.end();
    // return the fuel count
    new_func.local_get(fuel);

    // add the function to the `gen_wasm` and save the fid mapping
    let new_fid = new_func.finish_module(gen_wasm);

    // Export the function so it can be called externally
    // Gets named tyN, where:
    // - ty is the name of the type of gas calculation (exact or approximate)
    // - N is the original function's ID
    let fname = format!("{}{}{}", ty, orig_fid, spec_name);
    gen_wasm.exports.add_export_func(
        fname.clone(),
        *new_fid
    );
    generated_funcs.push(GeneratedFunc::new(*new_fid, fname, state));
}


// fn gen_func_old<'a, 'b>(fuel: &GlobalID, cost_map: &mut HashMap<usize, u64>, orig_fid: u32, start_idx: usize, body: &[Operator<'a>], slice: &mut SliceResult, ty: &CompType, gen_wasm: &mut Module<'b>, generated_funcs: &mut Vec<GeneratedFunc>) where 'a: 'b {
//     let (mut state, used_params) = CodeGen::new(slice);     // one instance of state per function!
//     let mut new_func = FunctionBuilder::new(&used_params, &[]);
//     for (i, op) in body.iter().enumerate().skip(start_idx) {
//         if handle_special(op) {
//             // TODO: SOMEWHERE IN HERE I NEED TO HANDLE A `loop` OPCODE
//             // probably gonna be recursively calling itself here!
//             todo!()
//
//             // how to skip over the opcodes that I've now processed?
//         }
//         let in_slice = slice.instrs.contains(&i);
//         let in_support = slice.instrs_support.contains(&i);
//         let do_fuel_before = calc_op_cost(in_slice | in_support, i == body.len() - 1, op, &mut state);
//
//         if do_fuel_before {
//             // Generate the fuel decrement
//             let cost = state.curr_cost;
//             gen_fuel_comp(&fuel, ty, &mut state, &mut new_func);
//             state.reset_cost();
//             cost_map.insert(i, cost);
//         }
//
//         if in_slice | in_support {
//             // Generate opcode that needs to be placed here in the generated function
//             gen_op(i, op, &state, &mut new_func);
//         }
//     }
//     // add the function to the `gen_wasm` and save the fid mapping
//     let new_fid = new_func.finish_module(gen_wasm);
//
//     // Export the function so it can be called externally
//     // Gets named tyN, where:
//     // - ty is the name of the type of gas calculation (exact or approximate)
//     // - N is the original function's ID
//     let fname = format!("{}{}", ty, orig_fid);
//     gen_wasm.exports.add_export_func(
//         fname.clone(),
//         *new_fid
//     );
//     generated_funcs.push(GeneratedFunc::new(*new_fid, fname, state));
// }

/// Returns: (should_include, do_fuel_before)
/// - support_opcode: whether this opcode should be included in the generated function.
/// - do_fuel_before: whether we should compute the fuel implications at this location
///   (before emitting this opcode).
fn calc_op_cost(is_in_slice: bool, at_func_end: bool, op: &Operator, state: &mut CodeGen) -> bool {
    // compute and increment the cost to calculate for this block
    state.add_cost(op_cost(op));

    let is_cf = is_branching_op(op) || matches!(op,
        Operator::If {..} |
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

fn gen_fuel_comp(fuel: &LocalID, ty: &CompType, state: &mut CodeGen, func: &mut FunctionBuilder) {
    match ty {
        CompType::Exact => gen_fuel_comp_exact(fuel, state, func),
        CompType::Approx => gen_fuel_comp_approx(fuel, state, func),
    }
}

fn gen_fuel_comp_exact(fuel: &LocalID, state: &mut CodeGen, func: &mut FunctionBuilder) {
    if state.curr_cost > 0 {
        func.local_get(*fuel);
        func.i64_const(state.curr_cost as i64);
        func.i64_add();
        func.local_set(*fuel);
    }
}

fn gen_fuel_comp_approx(_fuel: &LocalID, _state: &mut CodeGen, _func: &mut FunctionBuilder) {
    // TODO
    todo!()
}

// Translate instructions into `local.get` on parameter representing that state! (if necessary)
fn gen_op<'a, 'b>(opidx: usize, op: &Operator<'a>, fuel: &LocalID, gen_state: &CodeGen, func: &mut FunctionBuilder<'b>) where 'a : 'b {
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
        if let Operator::Return = op {
            func.local_get(*fuel);
        }
        func.inject(op.clone());
    }
}
