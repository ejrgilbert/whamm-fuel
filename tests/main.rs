use crate::utils::run_test;

mod utils;

// All of these test programs are expected to be located in the folder: `tests/programs`

#[test]
fn test_calls() {
    run_test("calls.wasm")
}
#[test]
fn test_globals() {
    run_test("globals.wasm")
}
#[test]
fn test_loads() {
    run_test("loads.wasm")
}
#[test]
fn test_params() {
    run_test("params.wasm")
}
