use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use wirm::{DataType, Opcode};
use wirm::ir::function::FunctionBuilder;
use wirm::ir::id::LocalID;
use crate::codegen::max::CodeGenMax;
use crate::codegen::min::CodeGenMin;

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
impl From<CodeGenMax> for GeneratedFunc {
    fn from(value: CodeGenMax) -> Self {
        let mut req_state = HashMap::new();
        req_state.insert(StateType::Param, value.for_params);
        req_state.insert(StateType::Global, value.for_globals);
        req_state.insert(StateType::Load, value.for_loads);
        req_state.insert(StateType::Call, value.for_calls);
        req_state.insert(StateType::CallIndirect, value.for_call_indirects);

        Self {
            fid: value.fid,
            fname: value.fname,
            req_state
        }
    }
}
impl From<CodeGenMin> for GeneratedFunc {
    fn from(value: CodeGenMin) -> Self {
        let mut req_state = HashMap::new();
        req_state.insert(StateType::Param, value.for_taken);

        Self {
            fid: value.fid,
            fname: value.fname,
            req_state
        }
    }
}

#[derive(Eq, PartialEq, Hash)]
pub enum StateType {
    Param,
    Global,
    Load,
    Call,
    CallIndirect,
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
