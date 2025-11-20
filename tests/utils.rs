use std::collections::HashMap;
use wasi_common::sync::{add_to_linker, WasiCtxBuilder};
use wasi_common::WasiCtx;
use wasmtime::{Engine, ExternType, FuncType, Instance, Linker, Module, Store, Val, ValType, V128};
use whamm_fuel::run::{do_analysis, FUEL_EXPORT, INIT_FUEL};
use whamm_fuel::run::CompType::{Approx, Exact};

const BASE_IN: &str = "tests/programs/";
const BASE_OUT: &str = "output/tests/";

pub fn run_test(fname: &str, on_true_vals: HashMap<u32, i64>, on_false_vals: HashMap<u32, i64>) {
    if let Err(e) = run_test_internal(fname, on_true_vals, on_false_vals) {
        panic!("Failed to run test `{}`\nError: {}", fname, e);
    }
}

fn run_test_internal(fname: &str, on_true_vals: HashMap<u32, i64>, on_false_vals: HashMap<u32, i64>) -> anyhow::Result<()> {
    let in_path = format!("{BASE_IN}{fname}");
    let out_path = format!("{BASE_OUT}{fname}");
    let bytes = std::fs::read(in_path)?;
    do_analysis(&bytes, &out_path)?;

    // 1. Is the output wasm file VALID?
    let engine = Engine::default();
    let wasm = test_validity(&engine, &out_path)?;

    // 2. Run the module, does it run as expected?
    for export in wasm.exports() {
        if let ExternType::Func(func_ty) = export.ty() {
            let name = export.name();
            if let Some(fid) = get_fid(name) {
                let exp_gas_true = on_true_vals.get(&fid).unwrap_or_else(|| {
                    panic!("Failed to find expected fuel value for function with FID: {fid}")
                });
                test_run(name, *exp_gas_true, gen_true, &func_ty, &engine, &wasm)?;

                let exp_gas_false = on_false_vals.get(&fid).unwrap_or_else(|| {
                    panic!("Failed to find expected fuel value for function with FID: {fid}")
                });
                test_run(name, *exp_gas_false, gen_false, &func_ty, &engine, &wasm)?;
            }
        }
    }

    Ok(())
}

fn test_validity(engine: &Engine, path: &str) -> anyhow::Result<Module> {
    Ok(Module::from_file(engine, path)?)
}

fn test_run(func_name: &str, exp_fuel: i64, gen_val: fn(ValType) -> Val, func_ty: &FuncType, engine: &Engine, wasm: &Module) -> anyhow::Result<()> {
    // Run each of the exported functions with some input to them (just generate values)
    // Is the output what I expect for each of these values?
    let (instance, mut store) = instantiate(engine, wasm)?;

    let mut args = Vec::new();
    // Optionally, get the function from the instance
    if let Some(func) = instance.get_func(&mut store, func_name) {
        for dt in func_ty.params() {
            args.push(gen_val(dt));
        }
        let mut results = Vec::new();
        func.call(&mut store, &mut args, &mut results)?;
    }

    // to check the fuel amount:
    let global = instance
        .get_global(&mut store, FUEL_EXPORT)
        .ok_or_else(|| anyhow::anyhow!("missing global"))?;

    let Val::I64(actual_fuel) = global.get(&mut store) else {
        Err(anyhow::anyhow!("expected fuel to be an i64"))?
    };
    assert_eq!(INIT_FUEL - exp_fuel, actual_fuel, "[{func_name}] fuel was not calculated correctly!\n\tRan with: {:?}", args);

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

fn get_fid(s: &str) -> Option<u32> {
    // Check for prefixes
    let prefixes = [Exact.to_string(), Approx.to_string()];
    for prefix in prefixes.iter() {
        if let Some(rest) = s.strip_prefix(prefix) {
            // Try to parse the rest as u32
            return rest.parse::<u32>().ok();
        }
    }
    None
}
