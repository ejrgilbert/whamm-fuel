use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::Write;
use termcolor::{ColorSpec, WriteColor};
use wasi_common::sync::{add_to_linker, WasiCtxBuilder};
use wasi_common::WasiCtx;
use wasmtime::{Engine, ExternType, FuncType, Instance, Linker, Module, Store, Val, ValType, V128};
use whamm_fuel::run::{do_analysis, CompType};
use whamm_fuel::run::CompType::{Approx, Exact};

const BASE_IN: &str = "tests/programs/";
const BASE_OUT: &str = "output/tests/";
const BASE_EXP: &str = "tests/programs/exp_out";

type FID = u32;
enum SliceType {
    Max,
    Min
}
impl Display for SliceType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SliceType::Max => write!(f, "max"),
            SliceType::Min => write!(f, "min"),
        }
    }
}

#[derive(Default)]
pub(crate) struct Test {
    name: &'static str,
    expected: HashMap<FID, TestCase>
}
impl Test {
    pub(crate) fn new(name: &'static str
    ) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }
    pub(crate) fn add_base_case(&mut self, fid: FID, base_max: Exp, base_min: Exp) {
        self.expected.insert(fid, TestCase::new(Expected::new(
            base_max,
            HashMap::default()
        ), Expected::new(
            base_min,
            HashMap::default()
        )));
    }
    pub(crate) fn add_case_with_loops(&mut self, fid: FID, base_max: Exp, loops_max: Vec<(LoopIdx, Exp)>,
                                      base_min: Exp, loops_min: Vec<(LoopIdx, Exp)>) {
        self.expected.insert(fid, TestCase::new(Expected::new(
            base_max,
            loops_max.into_iter().collect()
        ), Expected::new(
            base_min,
            loops_min.into_iter().collect()
        )));
    }
}

type LoopIdx = usize;
type Cost = i64;
pub struct Exp {
    exact_on_true: Cost,
    exact_on_false: Cost,
    approx_on_true: Cost,
    approx_on_false: Cost
}
impl Exp {
    pub fn new_exact(
        exact_on_true: Cost,
        exact_on_false: Cost
    ) -> Self {
        Self { exact_on_true, exact_on_false, approx_on_true: -1, approx_on_false: -1 }
    }
    pub fn new(
        exact_on_true: Cost,
        exact_on_false: Cost,
        approx_on_true: Cost,
        approx_on_false: Cost
    ) -> Self {
        Self { exact_on_true, exact_on_false, approx_on_true, approx_on_false }
    }
}

struct Expected {
    base: Exp,
    loops: HashMap<LoopIdx, Exp>
}
impl Expected {
    fn new(base: Exp, loops: HashMap<LoopIdx, Exp>) -> Self {
        Self { base, loops }
    }
}

struct TestCase {
    for_max: Expected,
    for_min: Expected
}
impl TestCase {
    pub(crate) fn new(for_max: Expected, for_min: Expected) -> Self {
        Self { for_max, for_min }
    }
    // fn get_loop_exp(&self, idx: LoopIdx) -> &Exp {
    //     self.loops.get(&idx).unwrap()
    // }
}

pub fn run_test(test_case: Test) {
    if let Err(e) = run_test_internal(&test_case) {
        panic!("Failed to run test `{}`\nError: {}", test_case.name, e);
    }
}

fn run_test_internal(test: &Test) -> anyhow::Result<()> {
    let in_path = format!("{BASE_IN}{}.wasm", test.name);
    let out_max_path = format!("{BASE_OUT}{}-max.wasm", test.name);
    let out_min_path = format!("{BASE_OUT}{}-min.wasm", test.name);
    let exp_path = format!("{BASE_EXP}/{}.wasm.out", test.name);
    let bytes = fs::read(in_path)?;

    let mut buf = TestBuffer { buf: Vec::new() };
    do_analysis(&mut buf, &bytes, &out_max_path, &out_min_path)?;

    // 0. Check the expected output information.
    println!("[test] Is output as expected?");
    let exp_output = fs::read_to_string(exp_path)?;
    let output = String::from_utf8(buf.buf)?;
    assert_eq!(output.trim(), exp_output.trim());

    // 1. Is the output wasm file VALID?
    println!("[test] Is it valid?");
    let engine = Engine::default();
    let wasm_max = test_validity(&engine, &out_max_path)?;
    let wasm_min = test_validity(&engine, &out_min_path)?;

    // 2. Run the module, does it run as expected?
    println!("[test] Does it run correctly?");
    run_wasm(SliceType::Max, test, &engine, wasm_max)?;
    run_wasm(SliceType::Min, test, &engine, wasm_min)?;

    Ok(())
}

fn test_validity(engine: &Engine, path: &str) -> anyhow::Result<Module> {
    Ok(Module::from_file(engine, path)?)
}

fn run_wasm(slice_ty: SliceType, test: &Test, engine: &Engine, wasm: Module) -> anyhow::Result<()> {
    let mut checked_loops_per_func: HashMap<u32, usize> = HashMap::default();
    for export in wasm.exports() {
        if let ExternType::Func(func_ty) = export.ty() {
            let name = export.name();
            if let Some((ty, fid, loop_idx)) = get_func_metadata(name) {
                let test_case = test.expected.get(&fid).unwrap();
                let Exp { exact_on_true: base_true, exact_on_false: base_false, .. } = if let Some(loop_idx) = loop_idx {
                    checked_loops_per_func.entry(fid).and_modify(|loops| {
                        *loops += 1;
                    }).or_insert(1);
                    match slice_ty {
                        SliceType::Max => &test_case.for_max.loops[&loop_idx],
                        SliceType::Min => &test_case.for_min.loops[&loop_idx]
                    }

                } else {
                    match slice_ty {
                        SliceType::Max => &test_case.for_max.base,
                        SliceType::Min => &test_case.for_min.base
                    }
                };
                test_run(name, &format!("{slice_ty}-on_true"), *base_true, gen_true, &func_ty, &engine, &wasm)?;
                test_run(name, &format!("{slice_ty}-on_false"), *base_false, gen_false, &func_ty, &engine, &wasm)?;
            }
        }
    }

    // check that we checked the expected number of generated loop slices.
    for (fid, case) in test.expected.iter() {
        let exp_count_max = case.for_max.loops.len();
        let exp_count_min = case.for_min.loops.len();
        assert_eq!(exp_count_max, exp_count_max);
        if exp_count_max > 0 {
            assert_eq!(exp_count_max, *checked_loops_per_func.get(&fid).unwrap());
        }
    }
    Ok(())
}

fn test_run(func_name: &str, case_name: &str, exp_fuel: i64, gen_val: fn(ValType) -> Val, func_ty: &FuncType, engine: &Engine, wasm: &Module) -> anyhow::Result<()> {
    // Run each of the exported functions with some input to them (just generate values)
    // Is the output what I expect for each of these values?
    let (instance, mut store) = instantiate(engine, wasm)?;

    let mut args = Vec::new();
    let mut results = vec![Val::I64(0)];
    // Optionally, get the function from the instance
    if let Some(func) = instance.get_func(&mut store, func_name) {
        for dt in func_ty.params() {
            args.push(gen_val(dt));
        }
        func.call(&mut store, &mut args, &mut results)?;
    }

    // to check the fuel amount:
    let Some(Val::I64(actual_fuel)) = results.get(0) else {
        Err(anyhow::anyhow!("expected fuel to be an i64"))?
    };
    assert_eq!(exp_fuel, *actual_fuel, "[{func_name}::{case_name}] fuel was not calculated correctly!\n\tRan with: {:?}", args);

    Ok(())
}

fn gen_true(ty: ValType) -> Val {
    gen_val(1, ty)
}
fn gen_false(ty: ValType) -> Val {
    gen_val(0, ty)
}

fn gen_val(literal: i32, ty: ValType) -> Val {
    match ty {
        ValType::I32 => Val::I32(literal),
        ValType::I64 => Val::I64(literal as i64),
        ValType::F32 => Val::F32(literal as u32),
        ValType::F64 => Val::F64(literal as u64),
        ValType::V128 => Val::V128(V128::from(literal as u128)),
        ValType::Ref(_) => todo!(),
    }
}

fn instantiate(engine: &Engine, wasm: &Module) -> anyhow::Result<(Instance, Store<WasiCtx>)> {
    // Provide WASI imports/store (if there are any); all instances in the store
    // share this context. `WasiCtxBuilder` provides a number of ways to
    // configure what the target program will have access to.
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .inherit_env()?
        .build();

    let mut store = Store::new(engine, wasi);

    // Set up a linker that knows about WASI
    let mut linker = Linker::new(engine);
    add_to_linker(&mut linker, |ctx: &mut WasiCtx| ctx)?;

    // Instantiate the module with the linker (this links in WASI)
    let instance = linker.instantiate(&mut store, wasm)?;

    Ok((instance, store))
}

fn get_func_metadata(s: &str) -> Option<(CompType, u32, Option<usize>)> {
    // Determine the type prefix
    let (ctype, rest) = if let Some(stripped) = s.strip_prefix("exact") {
        (Exact, stripped)
    } else if let Some(stripped) = s.strip_prefix("approx") {
        (Approx, stripped)
    } else {
        return None; // Unknown prefix
    };

    // Split on "_loop_at_" if it exists
    let parts: Vec<&str> = rest.split("_loop_at_").collect();

    // Parse the u32 immediately following the prefix
    let number = parts.get(0)?.parse::<u32>().ok()?;

    // Parse the optional loop number
    let loop_num = if parts.len() > 1 {
        Some(parts[1].parse::<usize>().ok()?)
    } else {
        None
    };

    Some((ctype, number, loop_num))
}

struct TestBuffer {
    buf: Vec<u8>,
}

impl Write for TestBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

impl WriteColor for TestBuffer {
    fn supports_color(&self) -> bool { false }   // tests: ignore colors
    fn set_color(&mut self, _spec: &ColorSpec) -> std::io::Result<()> { Ok(()) }
    fn reset(&mut self) -> std::io::Result<()> { Ok(()) }
}
