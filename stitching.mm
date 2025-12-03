// TODO -- check if Ben agrees with this idea and actually get this working in Whamm.

// TODO, new things to support:
// - fix how i pass strings to libraries
// - strings all the way
// - call function in global space
// - declare/set variable in global space
// - set variable in global space
// new events:
// - wasm:loop:back_edge
// new bound func/var:
// - prog_path
// - frame_vars (.push, .popi)
// - libcall()

// program.instr.wasm -> <async> / <memoization> -> slice
//  - bundling
//  - var cost = slice(arg...)
//  - fuel.consume(cost)

// This is the library that computes the cost for some program slice (per function).
// With this structure we can also put intermediate logic that looks up if we've previously
// invoked the slice with the same inputs (return the previously computed fuel cost).
use whamm_fuel;

// This is the domain-specific implementation of fuel for the host
// if it reaches 0, it does whatever it wants to to handle it.
use fuel;

whamm_fuel.analyze(prog_path);
var gen_fuel_path: string = whamm_fuel.gen_slice_min_path();

// Here’s an idea I have after our convo for how to write the whamm script. It’d include a new bound variable frame_vars that allows you to bundle state at different function locations.
// Then there’s a new bound function call that compiles to a Wasm call targeting the specified function and passing the specified vars (popped from the frame_vars stack):

wasm:opcode:*if* | br_table:before / whamm_fuel.instrument_here(fid, opidx) / {
    // `frame_vars`: a bound variable that is a stack that can be pushed/popped
    // one per function.
    // `taken`: a bound variable that says whether the branch was taken
    frame_vars.push(taken);
}
wasm:loop:back_edge / whamm_fuel.instrument_here(fid, startidx) / {
    // Get the name of the generated slice function for the current loop
    var lib_func: string = whamm_fuel.slice_func_loop(fid, startidx);
    // Get the number of parameters that the current loop's slice function takes
    var num_params: i32 = whamm_fuel.slice_params_loop(fid, startidx);
    
    // `libcall(target_lib, target_func, args)`: bound function that invokes the
    // specified library function with the passed vector of arguments
    var cost: i64 = libcall(gen_fuel_path, lib_func, frame_vars.popi(num_params)); // `popi` pops and returns a vector of the passed size
    fuel.consume(cost);
}
wasm:func:exit {
    // Get the name of the generated slice function for the current function
    var lib_func: string = whamm_fuel.slice_func(fid);
    // Get the number of parameters that the current function's slice function takes
    var num_params: i32 = whamm_fuel.slice_params(fid);
    
    var cost: i64 = libcall(gen_fuel_path, lib_func, frame_vars.popi(num_params));
    fuel.consume(cost);
}
