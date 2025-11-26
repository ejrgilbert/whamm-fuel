use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use wirm::{DataType, Module, Opcode};
use wirm::ir::function::FunctionBuilder;
use wirm::ir::id::{FunctionID, LocalID};
use wirm::ir::types::BlockType;
use wirm::module_builder::AddLocal;
use wirm::wasmparser::Operator;
use crate::analyze::FuncState;
use crate::run::CompType;
use crate::slice::{Slice, SliceResult};
use crate::utils::is_branching_op;

pub fn codegen<'a, 'b>(ty: &CompType, slices: &mut [SliceResult],
                       new_state: fn(&Slice) -> (CodeGenState, Vec<DataType>),
                       in_slice: fn(usize, &Slice) -> bool,
                       gen_op: fn(usize, &Operator<'a>, &LocalID, &CodeGenState, &mut FunctionBuilder<'b>),
                       funcs: &[FuncState], wasm: &Module<'a>, gen_wasm: &mut Module<'b>) -> CodeGenResult where 'a : 'b {
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

        let generated_funcs = gen_from_slices(func.fid, body.get_ops(), func_slices, new_state, in_slice, gen_op, &mut cost_map, ty, gen_wasm);
        func_map.insert(func.fid, generated_funcs);

        cost_maps.push(cost_map);
    }

    CodeGenResult {
        cost_maps,
        func_map
    }
}

fn gen_from_slices<'a, 'b>(orig_fid: u32, body: &[Operator<'a>], func_slices: &SliceResult,
                           new_state: fn(&Slice) -> (CodeGenState, Vec<DataType>),
                           in_slice: fn(usize, &Slice) -> bool,
                           gen_op: fn(usize, &Operator<'a>, &LocalID, &CodeGenState, &mut FunctionBuilder<'b>),
                           cost_map: &mut HashMap<usize, u64>, ty: &CompType, gen_wasm: &mut Module<'b>) -> Vec<GeneratedFunc> where 'a: 'b {
    let mut generated_funcs = vec![];

    let mut i = 0;
    while i < body.len() {
        if let Some(slice) = func_slices.slices.get(&i) {
            // I know I need to generate a function for this slice!
            let subsec = &body[slice.start_instr_idx..slice.end_instr_idx];
            gen_func(slice.start_instr_idx, &slice.spec_name, cost_map, orig_fid, subsec, slice, new_state, in_slice, gen_op, func_slices, ty, gen_wasm, &mut generated_funcs);
        }
        i += 1;
    }

    generated_funcs
}

fn gen_func<'a, 'b>(true_start_idx: usize, spec_name: &str, cost_map: &mut HashMap<usize, u64>, orig_fid: u32, body: &[Operator<'a>], slice: &Slice,
                    new_state: fn(&Slice) -> (CodeGenState, Vec<DataType>),
                    in_slice: fn(usize, &Slice) -> bool,
                    gen_op: fn(usize, &Operator<'a>, &LocalID, &CodeGenState, &mut FunctionBuilder<'b>),
                    func_slices: &SliceResult, ty: &CompType, gen_wasm: &mut Module<'b>, generated_funcs: &mut Vec<GeneratedFunc>) where 'a: 'b {
    let (mut state, used_params) = new_state(slice);     // one instance of state per function!
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

        let in_slice = in_slice(true_instr_idx, slice);
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
    state.fid = *new_fid;
    state.fname = fname.clone();

    generated_funcs.push(GeneratedFunc::from(state));
}

/// Returns: (should_include, do_fuel_before)
/// - support_opcode: whether this opcode should be included in the generated function.
/// - do_fuel_before: whether we should compute the fuel implications at this location
///   (before emitting this opcode).
fn calc_op_cost(is_in_slice: bool, at_func_end: bool, op: &Operator, state: &mut CodeGenState) -> bool {
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

fn gen_fuel_comp(fuel: &LocalID, ty: &CompType, state: &mut CodeGenState, func: &mut FunctionBuilder) {
    match ty {
        CompType::Exact => gen_fuel_comp_exact(fuel, state, func),
        CompType::Approx => gen_fuel_comp_approx(fuel, state, func),
    }
}

fn gen_fuel_comp_exact(fuel: &LocalID, state: &mut CodeGenState, func: &mut FunctionBuilder) {
    if state.curr_cost > 0 {
        func.local_get(*fuel);
        func.i64_const(state.curr_cost as i64);
        func.i64_add();
        func.local_set(*fuel);
    }
}

fn gen_fuel_comp_approx(_fuel: &LocalID, _state: &mut CodeGenState, _func: &mut FunctionBuilder) {
    // TODO
    todo!()
}

pub(crate) mod max;
pub(crate) mod min;

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

    // Maps from the type of state that we're requesting
    // to a HashMap from instr_idx -> stack values we need at that instr
    pub req_state: HashMap<StateType, HashMap<usize, ReqState>>,
}
impl From<CodeGenState> for GeneratedFunc {
    fn from(value: CodeGenState) -> Self {
        let mut req_state = HashMap::new();
        req_state.insert(StateType::Param, value.for_params);
        req_state.insert(StateType::Global, value.for_globals);
        req_state.insert(StateType::Load, value.for_loads);
        req_state.insert(StateType::Call, value.for_calls);
        req_state.insert(StateType::CallIndirect, value.for_call_indirects);
        req_state.insert(StateType::Taken, value.for_taken);

        Self {
            fid: value.fid,
            fname: value.fname,
            req_state
        }
    }
}


#[derive(Default)]
pub(crate) struct CodeGenState {
    pub(crate) fid: u32,
    pub(crate) fname: String,

    // Maps from dependency index -> generated local ID for each
    // of the types of program state the slice can depend on.
    pub(crate) for_params: HashMap<usize, ReqState>,
    pub(crate) for_globals: HashMap<usize, ReqState>,
    pub(crate) for_loads: HashMap<usize, ReqState>,
    pub(crate) for_calls: HashMap<usize, ReqState>,
    pub(crate) for_call_indirects: HashMap<usize, ReqState>,

    pub(crate) for_taken: HashMap<usize, ReqState>,

    // Used to track the current cost of the basic block
    // Once we reach a branching opcode, we need to gen the
    // cost computation before branching!
    // 1. generate computation
    // 2. curr_cost = 0
    curr_cost: u64
}
impl CodeGenState {
    fn new_max(slice: &Slice) -> (Self, Vec<DataType>) {
        let mut used_params = Vec::new();

        let for_params = process_needed_state(&slice.params.iter()
            .map(|((_, index), value)| (*index, value.clone()))
            .collect(), &mut used_params);
        let for_globals = process_needed_state(&slice.globals.iter()
            .map(|((_, index), value)| (*index, value.clone()))
            .collect(), &mut used_params);
        let for_loads = process_needed_state(&slice.loads, &mut used_params);
        let for_calls = process_needed_call(&slice.calls, &mut used_params);
        let for_call_indirects = process_needed_call(&slice.call_indirects, &mut used_params);

        fn process_needed_call(needed_state: &HashMap<(usize, usize), DataType>, used_params: &mut Vec<DataType>) -> HashMap<usize, ReqState> {
            let mut res = HashMap::default();
            for ((opidx, arg), dt) in needed_state.iter() {
                res.insert(*opidx, ReqState {
                    req_state: vec![ StackVal::Res { num: *arg, gen_param_id: used_params.len() as u32 }]
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
            ..Self::default()
        }, used_params)
    }
    fn new_min(slice: &Slice) -> (Self, Vec<DataType>) {
        let mut used_params = Vec::new();
        let for_taken = process_needed_state(&slice.taken, &mut used_params);
        (Self {
            for_taken,
            ..Self::default()
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

#[derive(Eq, PartialEq, Hash)]
pub enum StateType {
    Param,
    Global,
    Load,
    Call,
    CallIndirect,
    Taken
}
pub enum StackVal {
    Arg { num: usize, gen_param_id: u32 },
    Res { num: usize, gen_param_id: u32 },
}
impl StackVal {
    pub fn gen_param_id(&self) -> u32 {
        match self {
            StackVal::Arg { gen_param_id, .. } => *gen_param_id,
            StackVal::Res { gen_param_id, .. } => *gen_param_id,
        }
    }
}
impl Display for StackVal {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StackVal::Arg { num, gen_param_id } => { write!(f, "arg{num}@param{gen_param_id}") }
            StackVal::Res { num, gen_param_id } => { write!(f, "res{num}@param{gen_param_id}") }
        }
    }
}
pub struct ReqState { pub req_state: Vec<StackVal> }
impl Display for ReqState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut reqs = String::new();
        for (i, r) in self.req_state.iter().enumerate() {
            let comma = if i == 0 { "" } else { "," };
            reqs.push_str(&format!("{comma}{r}"));
        }
        write!(f, "{}", reqs)
    }
}

pub(crate) fn process_needed_state<T: Clone + Eq + Hash + Ord>(needed_state: &HashMap<T, DataType>, used_params: &mut Vec<DataType>) -> HashMap<T, ReqState> {
    let mut res = HashMap::default();
    let mut sorted: Vec<&T> = needed_state.keys().collect();
    sorted.sort();
    for key in sorted.iter() {
        let dt = needed_state.get(*key).unwrap();
        res.insert((*key).clone(), ReqState {
            req_state: vec![ StackVal::Res { num: 0, gen_param_id: used_params.len() as u32 }]
        });
        used_params.push(*dt);
    }
    res
}

fn handle_reqs<'a>(req_state: Option<&ReqState>, func: &mut FunctionBuilder<'a>) -> bool {
    if let Some(reqs) = req_state {
        for stack_val in reqs.req_state.iter() {
            func.local_get(LocalID(stack_val.gen_param_id()));
        }
        true
    } else {
        false
    }
}
