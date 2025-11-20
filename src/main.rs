mod run;
mod utils;
mod analyze;
mod slice;
mod codegen;

use anyhow::bail;
use termcolor::{ColorChoice, StandardStream};
use crate::run::do_analysis;

const OUTPUT: &str = "output.wasm";

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
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        bail!("Usage: whamm_fuel <file.wasm>");
    }
    let data = std::fs::read(&args[1])?;

    let stdout = StandardStream::stdout(ColorChoice::Always);
    do_analysis(stdout, &data, OUTPUT)?;
    Ok(())
}
