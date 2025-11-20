use std::collections::HashMap;
use crate::utils::run_test;

mod utils;

// NOTE: All of these test programs are expected to be located in the folder: `tests/programs`

#[test]
fn test_calls() {
    run_test("calls.wasm",
        HashMap::from(
        [(0, 2), (1, 5)]
        ),
        HashMap::from(
        [(0, 2), (1, 5)]
        ));
}
#[test]
fn test_globals() {
    run_test("globals.wasm",
        HashMap::from(
         [(0, 10)]
        ),
        HashMap::from(
         [(0, 11)]
        ));
}
#[test]
fn test_loads() {
    run_test("loads.wasm",
         HashMap::from(
         [(0, 6)]
         ),
         HashMap::from(
         [(0, 6)]
         ));
}
#[test]
fn test_params() {
    run_test("params.wasm",
        HashMap::from(
            [(0, 8),
                (1, 14),
                (2, 7),
                (3, 6),
                (4, 6),
                (5, 41),
                (6, 2)]
        ),
        HashMap::from(
            [(0, 9),
                (1, 9),
                (2, 7),
                (3, 6),
                (4, 6),
                (5, 41),
                (6, 2)]
        ));
}
