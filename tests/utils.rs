use wasmtime::Engine;
use whamm_fuel::run::do_analysis;

const BASE_IN: &str = "tests/programs/";
const BASE_OUT: &str = "tests/programs/";

pub fn run_test(fname: &str) {
    if let Err(e) = run_test_internal(fname) {
        panic!("Failed to run test `{}`\nError: {}", fname, e);
    }
}

fn run_test_internal(fname: &str) -> anyhow::Result<()> {
    let in_path = format!("{BASE_IN}{fname}");
    let out_path = format!("{BASE_OUT}{fname}");
    let bytes = std::fs::read(in_path)?;
    do_analysis(&bytes, &out_path)?;

    // 1. Is the output wasm file VALID?
    let engine = Engine::default();
    let _wasm = wasmtime::Module::from_file(&engine, out_path)?;

    // 2. Run each of the exported functions with some input to them (just generate values)
    //    Is the output what I expect for each of these values?
    // TODO

    Ok(())
}