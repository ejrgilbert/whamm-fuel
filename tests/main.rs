use std::collections::HashMap;
use crate::utils::run_test;

mod utils;

// NOTE: All of these test programs are expected to be located in the folder: `tests/programs`

#[test]
fn test_calls() {
    run_test("calls.wasm",
        HashMap::from(
        [(1, 1)]
        ),
        HashMap::from(
        [(2, 2)]
        ));
}
#[test]
fn test_globals() {
    run_test("globals.wasm",
        HashMap::from(
         [(1, 1)]
        ),
        HashMap::from(
         [(2, 2)]
        ));
}
#[test]
fn test_loads() {
    run_test("loads.wasm",
        HashMap::from(
        [(1, 1)]
        ),
        HashMap::from(
        [(2, 2)]
        ));
}
#[test]
fn test_params() {
    run_test("params.wasm",
        HashMap::from(
         [(1, 1)]
        ),
        HashMap::from(
         [(2, 2)]
        ));
}
