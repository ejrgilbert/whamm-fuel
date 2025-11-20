use crate::utils::{run_test, TestCase};

mod utils;

// NOTE: All of these test programs are expected to be located in the folder: `tests/programs`

#[test]
fn test_calls() {
    run_test(
        TestCase::new(
            "calls.wasm",
            vec![(0, 2), (1, 5)],
            vec![(0, 2), (1, 5)],
            vec![],
            vec![]
        )
    );
}
#[test]
fn test_globals() {
    run_test(
        TestCase::new(
            "globals.wasm",
            vec![(0, 10)],
            vec![(0, 11)],
            vec![],
            vec![]
        )
    );
}
#[test]
fn test_loads() {
    run_test(
        TestCase::new(
        "loads.wasm",
        vec![(0, 6)],
        vec![(0, 6)],
        vec![],
        vec![]
        )
    );
}
#[test]
fn test_params() {
    run_test(
        TestCase::new(
            "params.wasm",
            vec![(0, 8),
                (1, 14),
                (2, 7),
                (3, 6),
                (4, 6),
                (5, 41),
                (6, 2)],
            vec![(0, 9),
                (1, 9),
                (2, 7),
                (3, 6),
                (4, 6),
                (5, 41),
                (6, 2)],
            vec![],
            vec![]
        )
    );
}
