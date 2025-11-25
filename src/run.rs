use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::iter::zip;
use std::path::PathBuf;
use std::io::Write;
use std::str::FromStr;
use termcolor::{Color, ColorSpec, WriteColor};
use wirm::ir::id::FunctionID;
use wirm::{DataType, Module};
use crate::analyze::{analyze, FuncState};
use crate::codegen::{codegen, CallState, CodeGenResult, GeneratedFunc};
use crate::slice::{save_structure, slice_program, SliceResult};
use crate::utils::{FUEL_COMPUTATION, SPACE_PER_TAB};

pub enum CompType {
    Exact,
    Approx
}
impl Display for CompType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                CompType::Exact => "exact",
                CompType::Approx => "approx"
            }
        )
    }
}
impl FromStr for CompType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exact" => Ok(CompType::Exact),
            "approx" => Ok(CompType::Approx),
            _ => Err(format!("Unknown comp type: {}", s))
        }
    }
}

/// Compute backward slice of values that feed control-flow ops inside a function body.
/// - `num_params`: number of parameters (so we can mark `local.get` of param indices as Param).
pub fn do_analysis<W: WriteColor>(mut out: W, wasm_bytes: &[u8], out_path: &str) -> anyhow::Result<()> {
    // Read app Wasm into Wirm module
    let mut wasm = Module::parse(wasm_bytes, false, true).unwrap();

    let func_taints = analyze(&mut wasm);

    // create the slices
    let mut slices = slice_program(&func_taints, &wasm);
    save_structure(&mut slices, &func_taints, &wasm);

    // generate code for the slices (leave placeholders for the cost calculation)
    let mut gen_wasm = Module::default();
    let CodeGenResult { cost_maps, func_map } = codegen(&FUEL_COMPUTATION, &mut slices, &func_taints, &wasm, &mut gen_wasm);

    // Flush state
    flush_slices(&mut out, wasm.globals.len(), &slices, &func_taints, &cost_maps, &wasm)?;
    flush_fid_mapping(&mut out, &func_map)?;

    // Write the generated wasm to the output file
    write_bytes(&mut out, &gen_wasm.encode(), out_path)?;
    Ok(())
}

fn write_bytes<W: Write>(mut out: W, bytes: &[u8], out_path: &str) -> anyhow::Result<()> {
    writeln!(out, "\n====================")?;
    writeln!(out, "==== FLUSH WASM ====")?;
    writeln!(out, "====================")?;

    try_path(&out_path.to_string());
    if let Err(e) = std::fs::write(out_path, bytes) {
        unreachable!(
            "Failed to dump instrumented wasm to {} from error: {}",
            &out_path.to_string(), e
        )
    } else {
        writeln!(out, "Wrote generated Wasm to {}", out_path)?;
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

fn flush_fid_mapping<W: WriteColor>(mut out: W, fid_map: &HashMap<u32, Vec<GeneratedFunc>>) -> io::Result<()> {
    writeln!(out, "=====================")?;
    writeln!(out, "==== FID MAPPING ====")?;
    writeln!(out, "=====================")?;
    let mut sorted: Vec<&u32> = fid_map.keys().collect();
    sorted.sort();
    for fid in sorted.iter() {
        for GeneratedFunc {
            fid: new_fid,
            fname,
            for_params,
            for_globals,
            for_loads,
            for_calls,
            for_call_indirects
        } in fid_map.get(*fid).unwrap().iter() {
            let mut tabs = 0;
            write!(out, "{fid} -> ")?;
            print_fid(&mut out, &format!("{new_fid}:{fname}"));

            tabs += 1;
            print_params_for_state_req(&mut out, tabs, "LOCAL.GET (for a param)", for_params)?;
            print_params_for_state_req(&mut out, tabs, "GLOBAL.GET", for_globals)?;
            print_params_for_state_req(&mut out, tabs, "LOADS", for_loads)?;
            print_call_params_for_state_req(&mut out, tabs, "CALLS", for_calls)?;
            print_call_params_for_state_req(&mut out, tabs, "CALL_INDIRECTS", for_call_indirects)?;

            writeln!(out, )?;
        }

    }
    Ok(())
}

fn print_params_for_state_req<T: Debug, W: WriteColor>(mut out: W, tabs: i32, name: &str, map: &HashMap<T, u32>) -> io::Result<()> {
    if !map.is_empty() {
        writeln!(out, )?;
        writeln!(out, "{}---- Requested {name}:", tab(tabs))?;
        for (orig, new) in map.iter() {
            writeln!(out, "{}{:?} is @param{}", tab(tabs), orig, new)?;
        }
    }
    Ok(())
}
fn print_call_params_for_state_req<W: WriteColor>(mut out: W, tabs: i32, name: &str, map: &HashMap<usize, CallState>) -> io::Result<()> {
    if !map.is_empty() {
        writeln!(out, )?;
        writeln!(out, "{}---- Requested {name}:", tab(tabs))?;
        for (orig, CallState {used_arg, gen_param_id}) in map.iter() {
            writeln!(out, "{}{orig},arg{used_arg} is @param{gen_param_id}", tab(tabs))?;
        }
    }
    Ok(())
}

fn flush_slices<W: WriteColor>(mut out: W, num_globals: usize, slices: &Vec<SliceResult>, funcs: &Vec<FuncState>, cost_maps: &Vec<HashMap<usize, u64>>, wasm: &Module) -> io::Result<()> {
    writeln!(out, "\n================")?;
    writeln!(out, "==== SLICES ====")?;
    writeln!(out, "================")?;
    for (result, (func, cost_map)) in zip(slices, zip(funcs, cost_maps)) {
        let mut sorted: Vec<&usize> = result.slices.keys().collect();
        sorted.sort();
        for instr_index in sorted.iter() {
            let slice = &result.slices[*instr_index];

            writeln!(out, "function #{} ({} instructions in slice):", result.fid, slice.instrs.len())?;
            let body = &wasm.functions.unwrap_local(FunctionID(func.fid)).body.instructions;
            let mut tabs = 0;
            print_state_taint(&mut out, &slice.params, result.total_params, "params", &mut tabs)?;
            print_state_taint(&mut out, &slice.globals, num_globals, "global", &mut tabs)?;
            print_instr_taint(&mut out, &slice.params
                .iter()
                .map(|((_, index), value)| (*index, value.clone()))
                .collect(), "local.get", &mut tabs)?;
            print_instr_taint(&mut out, &slice.globals
                .iter()
                .map(|((_, index), value)| (*index, value.clone()))
                .collect(), "global.get", &mut tabs)?;
            print_instr_taint(&mut out, &slice.loads, "load", &mut tabs)?;
            print_call_taint(&mut out, &slice.calls, "calls", &mut tabs)?;
            print_call_taint(&mut out, &slice.call_indirects, "call_indirects", &mut tabs)?;


            tabs += 1;
            writeln!(out, "{}the function slice:", tab(tabs))?;
            tabs += 1;
            for i in 0..body.len() {
                let cost = cost_map.get(&i);
                let in_slice = slice.instrs.contains(&i);
                let in_support = slice.instrs_support.contains(&i);

                if let Some(cost) = cost {
                    let s = format!("{}\t! >>{cost}\n", tab(tabs));
                    print_cost(&mut out, &s);
                }

                let mark = if in_slice { "*" } else if in_support { "~" } else { " " };
                let s = format!("{}{}\t{} {:?}\n", tab(tabs), i, mark, body.get_ops().get(i).unwrap());
                if in_slice {
                    print_tainted(&mut out, &s);
                } else if in_support {
                    print_support(&mut out, &s);
                } else {
                    write!(out, "{s}")?;
                }
            }
            writeln!(out, )?;
        }
    }
    Ok(())
}
fn print_state_taint<W: WriteColor>(mut out: W, taint: &HashMap<(u32, usize), DataType>, out_of: usize, ty: &str, tabs: &mut i32) -> io::Result<()> {
    *tabs += 1;
    if !taint.is_empty() {
        writeln!(out, "{}the {ty} taint:", tab(*tabs))?;
        write!(out, "{}", tab(*tabs))?;

        let keys_u32: Vec<u32> = taint
            .keys()                // iterate over keys: &(u32, usize)
            .map(|(id, _)| *id)    // extract the u32 part
            .collect();

        for i in 0..out_of {
            let tainted = keys_u32.contains(&(i as u32));
            let s = format!(" {}{i},", if tainted { "*" } else { " " });
            if tainted {
                print_tainted(&mut out, &s);
            } else {
                write!(out, "{s}")?;
            }
        }
        writeln!(out, )?;
    }
    *tabs -= 1;
    Ok(())
}
fn print_instr_taint<W: WriteColor>(mut out: W, instrs: &HashMap<usize, DataType>, ty: &str, tabs: &mut i32) -> io::Result<()> {
    *tabs += 1;
    if !instrs.is_empty() {
        writeln!(out, "{}the {ty} instrs influencing CF:", tab(*tabs))?;
        write!(out, "{}", tab(*tabs))?;

        let mut sorted: Vec<&usize> = instrs.keys().collect();
        sorted.sort();
        for instr in sorted.iter() {
            print_tainted(&mut out, &format!(" *{},", **instr));
        }
        writeln!(out, )?;
    }
    *tabs -= 1;
    Ok(())
}
fn print_call_taint<W: WriteColor>(mut out: W, calls: &HashMap<(usize, usize), DataType>, ty: &str, tabs: &mut i32) -> io::Result<()> {
    *tabs += 1;
    if !calls.is_empty() {
        writeln!(out, "{}the {ty} instrs influencing CF:", tab(*tabs))?;
        write!(out, "{}", tab(*tabs))?;

        let mut sorted: Vec<&(usize, usize)> = calls.keys().collect();
        sorted.sort();
        for (instr, res) in sorted.iter() {
            print_tainted(&mut out, &format!(" *(@{}, res{}),", *instr, *res));
        }
        writeln!(out, )?;
    }
    *tabs -= 1;
    Ok(())
}

const WRITE_ERR: &str = "Uh oh, something went wrong while printing to terminal";

fn print_tainted<W: WriteColor>(out: W, s: &str) {
    print_color(out, s, green);
}
fn print_support<W: WriteColor>(out: W, s: &str) {
    print_color(out, s, blue);
}
fn print_cost<W: WriteColor>(out: W, s: &str) {
    print_color(out, s, red);
}
fn print_fid<W: WriteColor>(out: W, s: &str) {
    print_color(out, s, magenta_italics);
}
fn print_color<W: WriteColor>(out: W, s: &str, color: fn(W, bool, &str)) {
    color(out, true, s);
}
pub fn color<W: WriteColor>(mut out: W, s: &str, bold: bool, italics: bool, c: Color) {
    out
        .set_color(
            ColorSpec::new()
                .set_bold(bold)
                .set_italic(italics)
                .set_fg(Some(c)),
        )
        .expect(WRITE_ERR);
    write!(out, "{}", s).expect(WRITE_ERR);
    out.set_color(&ColorSpec::default()).expect("TODO: panic message");
}
pub fn blue<W: WriteColor>(out: W, bold: bool, s: &str) {
    color(out, s, bold, false, Color::Blue)
}
#[allow(dead_code)]
pub fn cyan<W: WriteColor>(out: W, bold: bool, s: &str) {
    color(out, s, bold, false, Color::Cyan)
}
pub fn green<W: WriteColor>(out: W, bold: bool, s: &str) {
    color(out, s, bold, false, Color::Green)
}
#[allow(dead_code)]
pub fn magenta<W: WriteColor>(out: W, bold: bool, s: &str) {
    color(out, s, bold, false, Color::Magenta)
}
pub fn magenta_italics<W: WriteColor>(out: W, bold: bool, s: &str) {
    color(out, s, bold, true, Color::Magenta)
}
pub fn red<W: WriteColor>(out: W, bold: bool, s: &str) {
    color(out, s, bold, false, Color::Red)
}
#[allow(dead_code)]
pub fn white<W: WriteColor>(out: W, bold: bool, s: &str) {
    color(out, s, bold, false, Color::Rgb(193, 193, 193))
}
#[allow(dead_code)]
pub fn grey_italics<W: WriteColor>(out: W, bold: bool, s: &str) {
    color(out, s, bold, true, Color::White)
}
#[allow(dead_code)]
pub fn yellow<W: WriteColor>(out: W, bold: bool, s: &str) {
    color(out, s, bold, false, Color::Yellow)
}
pub fn tab(tab: i32) -> String {
    " ".repeat(SPACE_PER_TAB * tab as usize)
}
