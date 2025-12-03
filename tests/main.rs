use crate::utils::{run_test, Exp, Test};

mod utils;

// NOTE: All of these test programs are expected to be located in the folder: `tests/programs`

#[test]
fn test_add() {
    let mut test = Test::new("add");
    test.add_base_case(
        0,
        Exp::new_exact(1, 1),
        Exp::new_exact(1, 1)
    );
    test.add_base_case(
        1,
        Exp::new_exact(4, 4),
        Exp::new_exact(4, 4)
    );

    run_test(test);
}

#[test]
fn test_calls() {
    let mut test = Test::new("calls");
    test.add_base_case(
        0,
        Exp::new_exact(2, 2),
        Exp::new_exact(2, 2)
    );
    test.add_base_case(
        1,
        Exp::new_exact(5, 5),
        Exp::new_exact(5, 5)
    );

    run_test(test);
}
#[test]
fn test_globals() {
    let mut test = Test::new("globals");
    test.add_case_with_loops(
        0,
        Exp::new_exact(4, 4),
        vec![(2, Exp::new_exact(6, 6))],
        Exp::new_exact(4, 4),
        vec![(2, Exp::new_exact(6, 6))]
    );
    run_test(test);
}
#[test]
fn test_loads() {
    let mut test = Test::new("loads");
    test.add_base_case(
        0,
        Exp::new_exact(6, 6),
        Exp::new_exact(6, 6)
    );
    run_test(test);
}

// TODO -- get this test case passing!
#[test]
fn test_malloc_init() {
    let mut test = Test::new("malloc_init");
    test.add_base_case(
        0,
        Exp::new_exact(2, 2),
        Exp::new_exact(2, 2)
    );
    test.add_base_case(
        1,
        Exp::new_exact(5, 5),
        Exp::new_exact(5, 5)
    );

    run_test(test);
}

#[test]
fn test_mem_ops() {
    let mut test = Test::new("mem-ops");
    test.add_base_case(
        0,
        Exp::new_exact(8, 8),
        Exp::new_exact(8, 8)
    );
    test.add_base_case(
        1,
        Exp::new_exact(2, 2),
        Exp::new_exact(2, 2)
    );

    run_test(test);
}

#[test]
fn test_mem_ops2() {
    let mut test = Test::new("mem-ops2");
    test.add_base_case(
        0,
        Exp::new_exact(8, 8),
        Exp::new_exact(8, 8)
    );

    run_test(test);
}
#[test]
fn test_params() {
    let mut test = Test::new("params");
    test.add_base_case(
        0,
        Exp::new_exact(8, 9),
        Exp::new_exact(8, 9)
    );
    test.add_base_case(
        1,
        Exp::new_exact(14, 9),
        Exp::new_exact(9, 14)
    );
    test.add_base_case(
        2,
        Exp::new_exact(7, 7),
        Exp::new_exact(7, 7)
    );
    test.add_base_case(
        3,
        Exp::new_exact(6, 6),
        Exp::new_exact(6, 6)
    );
    test.add_base_case(
        4,
        Exp::new_exact(6, 6),
        Exp::new_exact(6, 6)
    );
    test.add_base_case(
        5,
        Exp::new_exact(41, 41),
        Exp::new_exact(41, 41)
    );
    test.add_base_case(
        6,
        Exp::new_exact(2, 2),
        Exp::new_exact(2, 2)
    );
    run_test(test);
}

#[test]
fn test_params_edge1() {
    let mut test = Test::new("params-edge1");
    test.add_base_case(
        0,
        Exp::new_exact(3, 3),
        Exp::new_exact(3, 3)
    );

    // The input conditional gets flipped! The way we're passing
    // state is incorrect, it should be pulled from the
    // local.get/global.get directly! Not the eventual value!
    test.add_base_case(
        1,
        Exp::new_exact(7, 11),
        Exp::new_exact(7, 11)
    );
    test.add_base_case(
        2,
        Exp::new_exact(3, 3),
        Exp::new_exact(3, 3)
    );
    run_test(test);
}