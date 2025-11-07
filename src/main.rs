use anyhow::{Result, bail};
use std::collections::{HashSet, VecDeque};
use wirm::iterator::iterator_trait::Iterator;
use wirm::iterator::module_iterator::ModuleIterator;
use wirm::{Location, Module};
use wirm::ir::id::{FunctionID, TypeID};
use wirm::ir::module::module_types::Types;
use wirm::wasmparser::{BlockType, Operator};
use std::io::Write;
use termcolor::{Buffer, BufferWriter, Color, ColorChoice, ColorSpec, WriteColor};

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
    /// Call at instruction index
    Call {
        instr_idx: usize
    },
    /// A constant literal
    Const {
        instr_idx: usize
    },
    /// Unknown / external / untracked
    Unknown
}

/// Lightweight kind of operator we care about for slicing & identification.
#[derive(Debug, Clone)]
enum OpKind {
    Control,      // br_if, if, br_table, br, select (select we treat specially)
    Load,         // any memory load
    Store,        // memory store (we don't treat as sources but stack effects matter)
    GlobalGet(u32),
    GlobalSet(u32),
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    Const,
    Binary,
    Unary,
    Call,           // simplified: consumes args, produces result (we won't analyze inside)
    CallIndirect,   // simplified: consumes args, produces result (we won't analyze inside)
    Other,
}

/// Record for each instruction we saw.
#[derive(Debug)]
struct InstrInfo {
    idx: usize,
    operator: String,
    kind: OpKind,
    /// immediate origins used as inputs by this instruction (in order popped)
    inputs: Vec<Origin>,
    /// how many values it produced (we don't keep per-output origins here;
    /// produced origins are always `Origin::Instr(idx)` for each output).
    produces: usize,
}

/// Result of the slice analysis.
#[derive(Debug)]
struct SliceResult {
    fid: u32,
    total_params: usize,
    /// all instruction indices that are in the backward slice (influencing control).
    instrs: HashSet<usize>,
    /// function parameter indices that influence control
    params: HashSet<u32>,
    /// global indices (global.get) that influence control
    globals: HashSet<u32>,
    /// load instruction indices that influence control
    loads: HashSet<usize>,
    /// call instruction indices that influence control
    calls: HashSet<usize>,
}

#[derive(Default)]
struct FuncTaint {
    fid: u32,
    // current origin of each local (local index -> Origin). Locals include params + locals.
    // At start, parameters are available through local.get (we treat local.get of < num_params as Param).
    local_origin: Vec<Origin>,
    total_params: usize,
    total_results: usize,

    // To track the program slice we'll build
    slice_offsets: HashSet<usize>,

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
        let mut num_locals = total_params;
        let func = wasm.functions.unwrap_local(fid);
        for (i, _) in func.body.locals.iter() {
            num_locals += *i as usize;
        }

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
    let slices = slice(&func_taints);
    flush(wasm.globals.len(), &slices, &func_taints);

    Ok(())
}

fn analyze(wasm: &mut Module) -> Vec<FuncTaint>{
    let mut mi = ModuleIterator::new(wasm, &vec![]);
    let mut func_taints: Vec<FuncTaint> = Vec::new();

    let mut first = true;
    let mut state = FuncTaint::default();
    while first || mi.next().is_some() {
        let (
            Location::Module {func_idx, instr_idx} |
            Location::Component {func_idx, instr_idx, ..},
            at_func_end
        ) = mi.curr_loc();
        println!("Function #{} at instruction offset: {}", *func_idx, instr_idx);

        if instr_idx == 0 {
            // we're at the start of a new function! --> reset state
            if !first {
                // only save if this isn't the first function we're visiting
                assert!(state.stack.len() == state.total_results || state.stack.is_empty(), "still had stack values leftover: {:?}", state.stack);
                func_taints.push(state);
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
                    operator: format!("{:?}", op),
                    kind: OpKind::Const,
                    inputs: vec![],
                    produces: 1,
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
                            Origin::Unknown
                        }
                    });
                state.stack.push(origin.clone());
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
                    operator: format!("{:?}", op),
                    kind: OpKind::LocalGet(*local_index),
                    inputs: vec![], // origin already recorded on stack
                    produces: 1,
                });
            }

            Operator::LocalSet { local_index } => {
                // consumes one value and stores into local
                let val = state.stack.pop().unwrap();
                // update local origin
                state.local_origin[*local_index as usize] = val.clone();
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
                    operator: format!("{:?}", op),
                    kind: OpKind::LocalSet(*local_index),
                    inputs: vec![val],
                    produces: 0,
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
                    operator: format!("{:?}", op),
                    kind: OpKind::LocalTee(*local_index),
                    inputs: vec![val],
                    produces: 1,
                });
            }

            // ---------------- Globals ----------------
            Operator::GlobalGet { global_index } => {
                state.stack.push(Origin::Global {instr_idx, gid: *global_index});
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
                    operator: format!("{:?}", op),
                    kind: OpKind::GlobalGet(*global_index),
                    inputs: vec![],
                    produces: 1,
                });
            }

            Operator::GlobalSet { global_index } => {
                let val = state.stack.pop().unwrap();
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
                    operator: format!("{:?}", op),
                    kind: OpKind::GlobalSet(*global_index),
                    inputs: vec![val],
                    produces: 0,
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
                    operator: format!("{:?}", op),
                    kind: OpKind::Load,
                    inputs: vec![addr_origin],
                    produces: 1,
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
                    operator: format!("{:?}", op),
                    kind: OpKind::Store,
                    inputs: vec![addr, val],
                    produces: 0,
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
                    operator: format!("{:?}", op),
                    kind: OpKind::Binary,
                    inputs: vec![a, b],
                    produces: 1,
                });
            }

            Operator::I32Eqz { .. } | Operator::I32Clz { .. } => {
                let a = state.stack.pop().unwrap();
                state.stack.push(Origin::Instr {instr_idx});
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
                    operator: format!("{:?}", op),
                    kind: OpKind::Unary,
                    inputs: vec![a],
                    produces: 1,
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
                    operator: format!("{:?}", op),
                    kind: OpKind::Control, // treat select as control-influencing (it's a conditional)
                    inputs: vec![val1, val2, cond],
                    produces: 1,
                });
            }

            // ---------------- Branch / Control ----------------
            Operator::BrIf { .. } | Operator::If { .. } => {
                // pops condition
                let cond = state.stack.pop().unwrap();
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
                    operator: format!("{:?}", op),
                    kind: OpKind::Control,
                    inputs: vec![cond],
                    produces: 0,
                });
            }

            // ---------------- Calls ----------------
            // We don't inspect callee internals here; pop nargs (unknown) and push result (if any).
            Operator::Call { function_index } => {
                let tid = mi.module.functions.get(FunctionID(*function_index)).get_type_id();
                let (pops, pushes) = if let Some(Types::FuncType { params , results, ..}) = mi.module.types.get(tid) {
                    (params.len(), results.len())
                } else {
                    panic!("Should have found a function type!");
                };
                // conservative: assume 1 arg popped and 1 result produced (not precise)
                // ideally, use type information to know the real parameter count and results
                let mut inputs = Vec::new();
                for _ in 0..pops {
                    inputs.push(state.stack.pop().unwrap());
                }
                for _ in 0..pushes {
                    state.stack.push(Origin::Call {instr_idx})
                }
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
                    operator: format!("{:?}", op),
                    kind: OpKind::Call,
                    inputs,
                    produces: 1,
                });
            }
            Operator::CallIndirect {..} => todo!(),

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
                    inputs.push(state.stack.pop().unwrap());
                }

                for _ in 0..pushes {
                    state.stack.push(Origin::Unknown)
                }
                state.instrs.push(InstrInfo {
                    idx: instr_idx,
                    operator: format!("{:?}", op),
                    kind: OpKind::Other,
                    inputs,
                    produces: 0,
                });
            }
        }
    }
    // push the state of the final function
    assert!(state.stack.len() == state.total_results || state.stack.is_empty(), "still had stack values leftover: {:?}", state.stack);
    func_taints.push(state);

    func_taints
}

// Determine pops/pushes for instruction
// returns (pops, pushes)
fn stack_effects(op: &Operator, wasm: &Module) -> (usize, usize) {
    // TODO -- work with all operators!
    return match op {
        Operator::If { blockty, .. } => {
            block_effects(1, &blockty, wasm)
        },
        Operator::Block {blockty, ..} => {
            block_effects(0, &blockty, wasm)
        },
        Operator::BrIf { .. } |
        Operator::BrTable { .. } => (1, 0),
        Operator::Call { function_index } => {
            let tid = wasm.functions.get(FunctionID(*function_index)).get_type_id();
            ty_effects(0, *tid, wasm)
        }
        Operator::CallIndirect { type_index, .. } => ty_effects(1, *type_index, wasm),
        Operator::LocalGet { .. } => (0, 1),
        Operator::LocalSet { .. } => (1, 0),
        Operator::LocalTee { .. } => (1, 1),
        Operator::GlobalGet { .. } => (0, 1),
        Operator::GlobalSet { .. } => (1, 0),
        Operator::I32Const { .. } | Operator::I64Const { .. } | Operator::F32Const { .. } | Operator::F64Const { .. } => (0,1),
        Operator::I32Load { .. } | Operator::I64Load { .. } | Operator::F32Load { .. } | Operator::F64Load { .. } => (1,1),
        Operator::I32Store { .. } | Operator::I64Store { .. } | Operator::F32Store { .. } | Operator::F64Store { .. } => (2,0),
        Operator::Drop { .. } => (1, 0),
        _ => (0,0),
    };

    fn block_effects(extra_pop: usize, blockty: &BlockType, wasm: &Module) -> (usize, usize) {
        match blockty {
            BlockType::Empty => (extra_pop, 0),
            BlockType::Type(_) => (extra_pop, 1),
            BlockType::FuncType(tid) => ty_effects(extra_pop, *tid, wasm),
        }
    }
    fn ty_effects(extra_pop: usize, tid: u32, wasm: &Module) -> (usize, usize) {
        if let Some(Types::FuncType { params , results, ..}) = wasm.types.get(TypeID(tid)) {
            (params.len() + extra_pop, results.len())
        } else {
            panic!("Should have found a function type!");
        }
    }
}

fn slice(func_taints: &Vec<FuncTaint>) -> Vec<SliceResult> {
    let mut slices = Vec::new();
    for (i, taint) in func_taints.iter().enumerate() {
        slices.push(slice_func(taint));
    }
    slices
}

fn slice_func(state: &FuncTaint) -> SliceResult {
    // Start from control instructions' inputs
    let mut worklist: VecDeque<Origin> = VecDeque::new();
    let mut included_instrs: HashSet<usize> = HashSet::new();
    let mut included_params: HashSet<u32> = HashSet::new();
    let mut included_globals: HashSet<u32> = HashSet::new();
    let mut included_loads: HashSet<usize> = HashSet::new();
    let mut included_calls: HashSet<usize> = HashSet::new();

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

            Origin::Call {instr_idx} => {
                // Mark the call itself as influencing control
                if !included_calls.insert(instr_idx) {
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

            Origin::Unknown => {
                // nothing to trace further
            }
        }
    }

    SliceResult {
        fid: state.fid,
        total_params: state.total_params,
        instrs: included_instrs,
        params: included_params,
        globals: included_globals,
        loads: included_loads,
        calls: included_calls
    }
}

fn flush(num_globals: usize, slices: &Vec<SliceResult>, funcs: &Vec<FuncTaint>) {
    println!("\n===================");
    println!("==== FUNCTIONS ====");
    println!("===================");
    for (slice, func) in slices.iter().zip(funcs.iter()) {
        println!("function #{} ({} instructions):", slice.fid, slice.instrs.len());
        let mut tabs = 0;
        print_state_taint(&slice.params, slice.total_params, "params", &mut tabs);
        print_state_taint(&slice.globals, num_globals, "global", &mut tabs);
        print_instr_taint(&slice.loads, "load", &mut tabs);
        print_instr_taint(&slice.calls, "calls", &mut tabs);
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

        tabs += 1;
        println!("{}the function slice:", tab(tabs));
        tabs += 1;
        for (i, instr_info) in func.instrs.iter().enumerate() {
            let in_slice = slice.instrs.contains(&i);
            let s = format!("{}{}\t{} {}\n", tab(tabs), instr_info.idx, if in_slice { "*" } else { " " }, instr_info.operator);
            if in_slice {
                print_tainted(&s);
            } else {
                print!("{s}");
            }
        }
        tabs -= 1;
        tabs -= 1;
        println!();
    }
}

fn print_tainted(s: &str) {
    let writer = BufferWriter::stdout(ColorChoice::Always);
    let mut buff = writer.buffer();
    green(true, s, &mut buff);
    writer
        .print(&buff)
        .expect("Uh oh, something went wrong while printing to terminal");
}

// ===========================
// = Terminal Printing Logic =
// ===========================

const WRITE_ERR: &str = "Uh oh, something went wrong while printing to terminal";

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
