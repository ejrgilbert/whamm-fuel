use crate::utils::{run_test, Exp, Test};

mod utils;

// NOTE: All of these test programs are expected to be located in the folder: `tests/programs`

#[test]
fn test_add() {
    let mut test = Test::new("add.wasm");
    test.add_base_case(
        0,
        Exp::new_exact(1, 1)
    );
    test.add_base_case(
        1,
        Exp::new_exact(4, 4)
    );

    run_test(test);
}

#[test]
fn test_calls() {
    let mut test = Test::new("calls.wasm");
    test.add_base_case(
        0,
        Exp::new_exact(2, 2)
    );
    test.add_base_case(
        1,
        Exp::new_exact(5, 5)
    );

    run_test(test);
}
#[test]
fn test_globals() {
    // TODO -- add tests to exercise the loop subsections!
    let mut test = Test::new("globals.wasm");
    test.add_case_with_loops(
        0,
        Exp::new_exact(4, 4),
        vec![(2, Exp::new_exact(6, 6))]
    );
    run_test(test);
}
#[test]
fn test_loads() {
    let mut test = Test::new("loads.wasm");
    test.add_base_case(
        0,
        Exp::new_exact(6, 6)
    );
    run_test(test);
}
#[test]
fn test_params() {
    let mut test = Test::new("params.wasm");
    test.add_base_case(
        0,
        Exp::new_exact(8, 9)
    );
    test.add_base_case(
        1,
        Exp::new_exact(14, 9)
    );
    test.add_base_case(
        2,
        Exp::new_exact(7, 7)
    );
    test.add_base_case(
        3,
        Exp::new_exact(6, 6)
    );
    test.add_base_case(
        4,
        Exp::new_exact(6, 6)
    );
    test.add_base_case(
        5,
        Exp::new_exact(41, 41)
    );
    test.add_base_case(
        6,
        Exp::new_exact(2, 2)
    );
    run_test(test);
}

// TODO -- get this test case passing!
#[test]
fn test_malloc_init() {
    let mut test = Test::new("malloc_init.wasm");
    test.add_base_case(
        0,
        Exp::new_exact(2, 2)
    );
    test.add_base_case(
        1,
        Exp::new_exact(5, 5)
    );

    run_test(test);
}

#[test]
fn test_mem_ops() {
    let mut test = Test::new("mem-ops.wasm");
    test.add_base_case(
        0,
        Exp::new_exact(8, 8)
    );
    test.add_base_case(
        1,
        Exp::new_exact(2, 2)
    );

    run_test(test);
}

#[test]
fn test_mem_ops2() {
    let mut test = Test::new("mem-ops2.wasm");
    test.add_base_case(
        0,
        Exp::new_exact(8, 8)
    );

    run_test(test);
}
