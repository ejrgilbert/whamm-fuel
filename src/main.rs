mod utils;
mod analyze;
mod slice;
mod codegen;

use anyhow::{Result, bail};
use std::collections::HashMap;
use std::fmt::Debug;
use wirm::{DataType, Module};
use wirm::ir::id::{FunctionID};
use std::io::Write;
use std::iter::zip;
use std::path::PathBuf;
use termcolor::{Buffer, BufferWriter, Color, ColorChoice, ColorSpec, WriteColor};
use crate::analyze::{analyze, FuncState};
use crate::codegen::{codegen, CallState, CodeGenResult, CompType, GeneratedFunc};
use crate::slice::{save_structure, slice, SliceResult};

const OUTPUT: &str = "output.wasm";
const INIT_FUEL: i64 = 1000;
const FUEL_COMPUTATION: CompType = CompType::Exact;
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
///
/// Things to configure per domain:
/// - The amount of initial fuel allotted to computation (configured with INIT_FUEL)
/// - The fuel cost per opcode (see codegen::op_cost function)
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        bail!("Usage: wasm_taint_slice <file.wasm>");
    }
    let data = std::fs::read(&args[1])?;
    do_analysis(&data)?;
    Ok(())
}

/// Compute backward slice of values that feed control-flow ops inside a function body.
/// - `num_params`: number of parameters (so we can mark `local.get` of param indices as Param).
fn do_analysis(wasm_bytes: &[u8]) -> Result<()> {
    // Read app Wasm into Wirm module
    let mut wasm = Module::parse(wasm_bytes, false, true).unwrap();

    let func_taints = analyze(&mut wasm);

    // create the slices
    let mut slices = slice(&func_taints, &wasm);
    save_structure(&mut slices, &func_taints, &wasm);

    // generate code for the slices (leave placeholders for the cost calculation)
    let mut gen_wasm = Module::default();
    let CodeGenResult { cost_maps, func_map } = codegen(&FUEL_COMPUTATION, &mut slices, &func_taints, &wasm, &mut gen_wasm);

    // Flush state
    flush_slices(wasm.globals.len(), &slices, &func_taints, &cost_maps, &wasm);
    flush_fid_mapping(&func_map);

    // Write the generated wasm to the output file
    write_bytes(&gen_wasm.encode())
}

fn write_bytes(bytes: &[u8]) -> Result<()> {
    println!("\n====================");
    println!("==== FLUSH WASM ====");
    println!("====================");

    try_path(&OUTPUT.to_string());
    if let Err(e) = std::fs::write(OUTPUT, bytes) {
        unreachable!(
            "Failed to dump instrumented wasm to {} from error: {}",
            &OUTPUT.to_string(), e
        )
    } else {
        println!("Wrote generated Wasm to {}", OUTPUT);
    }
    Ok(())
}

pub(crate) fn try_path(path: &String) {
    if !PathBuf::from(path).exists() {
        std::fs::create_dir_all(PathBuf::from(path).parent().unwrap()).unwrap();
    }
}

// ===========================
// = Terminal Printing Logic =
// ===========================

fn flush_fid_mapping(fid_map: &HashMap<u32, GeneratedFunc>) {
    println!("=====================");
    println!("==== FID MAPPING ====");
    println!("=====================");
    let mut sorted: Vec<&u32> = fid_map.keys().collect();
    sorted.sort();
    for fid in sorted.iter() {
        let mut tabs = 0;
        let GeneratedFunc {
            fid: new_fid,
            for_params,
            for_globals,
            for_loads,
            for_calls,
            for_call_indirects
        } = fid_map.get(*fid).unwrap();
        print!("{fid} -> ");
        print_fid(&format!("{}", new_fid));

        tabs += 1;
        print_params_for_state_req(tabs, "PARAMS", for_params);
        print_params_for_state_req(tabs, "GLOBALS", for_globals);
        print_params_for_state_req(tabs, "LOADS", for_loads);
        print_call_params_for_state_req(tabs, "CALLS", for_calls);
        print_call_params_for_state_req(tabs, "CALL_INDIRECTS", for_call_indirects);

        println!();
    }
    fn print_params_for_state_req<T: Debug>(tabs: i32, name: &str, map: &HashMap<T, u32>) {
        if !map.is_empty() {
            println!();
            println!("{}---- Requested {name}:", tab(tabs));
            for (orig, new) in map.iter() {
                println!("{}{:?} is @param{}", tab(tabs), orig, new);
            }
        }
    }
    fn print_call_params_for_state_req(tabs: i32, name: &str, map: &HashMap<usize, CallState>) {
        if !map.is_empty() {
            println!();
            println!("{}---- Requested {name}:", tab(tabs));
            for (orig, CallState {used_arg, gen_param_id}) in map.iter() {
                println!("{}{orig},arg{used_arg} is @param{gen_param_id}", tab(tabs));
            }
        }
    }
}

fn flush_slices(num_globals: usize, slices: &Vec<SliceResult>, funcs: &Vec<FuncState>, cost_maps: &Vec<HashMap<usize, u64>>, wasm: &Module) {
    println!("\n================");
    println!("==== SLICES ====");
    println!("================");
    for (slice, (func, cost_map)) in zip(slices, zip(funcs, cost_maps)) {
        println!("function #{} ({} instructions in slice):", slice.fid, slice.instrs.len());
        let body = &wasm.functions.unwrap_local(FunctionID(func.fid)).body.instructions;
        let mut tabs = 0;
        print_state_taint(&slice.params, slice.total_params, "params", &mut tabs);
        print_state_taint(&slice.globals, num_globals, "global", &mut tabs);
        print_instr_taint(&slice.loads, "load", &mut tabs);
        print_call_taint(&slice.calls, "calls", &mut tabs);
        print_call_taint(&slice.call_indirects, "call_indirects", &mut tabs);
        fn print_state_taint(taint: &HashMap<u32, DataType>, out_of: usize, ty: &str, tabs: &mut i32) {
            *tabs += 1;
            if !taint.is_empty() {
                println!("{}the {ty} taint:", tab(*tabs));
                print!("{}", tab(*tabs));

                for i in 0..out_of {
                    let tainted = taint.contains_key(&(i as u32));
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
        fn print_instr_taint(instrs: &HashMap<usize, DataType>, ty: &str, tabs: &mut i32) {
            *tabs += 1;
            if !instrs.is_empty() {
                println!("{}the {ty} instrs influencing CF:", tab(*tabs));
                print!("{}", tab(*tabs));

                let mut sorted: Vec<&usize> = instrs.keys().collect();
                sorted.sort();
                for instr in sorted.iter() {
                    print_tainted(&format!("*{}, ", **instr));
                }
                println!();
            }
            *tabs -= 1;
        }
        fn print_call_taint(calls: &HashMap<(usize, usize), DataType>, ty: &str, tabs: &mut i32) {
            *tabs += 1;
            if !calls.is_empty() {
                println!("{}the {ty} instrs influencing CF:", tab(*tabs));
                print!("{}", tab(*tabs));

                let mut sorted: Vec<&(usize, usize)> = calls.keys().collect();
                sorted.sort();
                for (instr, res) in sorted.iter() {
                    print_tainted(&format!("*(@{}, res{}), ", *instr, *res));
                }
                println!();
            }
            *tabs -= 1;
        }

        tabs += 1;
        println!("{}the function slice:", tab(tabs));
        tabs += 1;
        for i in 0..body.len() {
            let cost = cost_map.get(&i);
            let in_slice = slice.instrs.contains(&i);
            let in_support = slice.instrs_support.contains(&i);

            if let Some(cost) = cost {
                let s = format!("{}\t! >>{cost}\n", tab(tabs));
                print_cost(&s);
            }

            let mark = if in_slice { "*" } else if in_support { "~" } else { " " };
            let s = format!("{}{}\t{} {:?}\n", tab(tabs), i, mark, body.get_ops().get(i).unwrap());
            if in_slice {
                print_tainted(&s);
            } else if in_support {
                print_support(&s);
            } else {
                print!("{s}");
            }
        }
        println!();
    }
}

const WRITE_ERR: &str = "Uh oh, something went wrong while printing to terminal";

fn print_tainted(s: &str) {
    print_color(s, green);
}
fn print_support(s: &str) {
    print_color(s, blue);
}
fn print_cost(s: &str) {
    print_color(s, red);
}
fn print_fid(s: &str) {
    print_color(s, magenta_italics);
}
fn print_color(s: &str, color: fn(bool, &str, &mut Buffer)) {
    let writer = BufferWriter::stdout(ColorChoice::Always);
    let mut buff = writer.buffer();
    color(true, s, &mut buff);
    writer
        .print(&buff)
        .expect("Uh oh, something went wrong while printing to terminal");
}
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
