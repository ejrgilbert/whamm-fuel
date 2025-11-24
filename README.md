# `whamm-fuel` # 

This is a repository containing currently in-flux code as we work toward building efficient fuel monitoring using the
[`whamm`](https://github.com/ejrgilbert/whamm) instrumentation framework for WebAssembly.

# TODO List #

- [ ] Add more tests! (maybe just assert they're valid for now?)
- [ ] Get working e2e (with whamm instrumentation)
      - Should codegen a whamm script (do we need new language features?)
      - Should it return the fuel cost? Decrement handled in `whamm` (gets reported automatically)
      - Create a test framework that actually performs this and tests that it works
- [ ] Get this working for APPROXIMATE fuel calculations
- [ ] `async` analysis (how to buffer events? likely similar to buffering of state in `whamm` invocation above)
- [ ] offline analysis (after finishes executing)
- [ ] Extend to support ALL Wasm opcodes (right now it just supports Wasm CORE opcodes)

# CodeGen a `Whamm` script #

Design for the script that will stitch the calls together. I need to:
1. Bundle state as I follow a function's execution
   - For base function cost slice invocaton
   - For inner loop cost slice invocation
     - Inject call to this loop with its bundled state for all backedges!
2. Invoke the appropriate slice with the bundled state

```
// This is the library that computes the cost for some program slice (per function).
// With this structure we can also put intermediate logic that looks up if we've previously
// invoked the slice with the same inputs (return the previously computed fuel cost).
use whamm_fuel;

// This is the domain-specific implementation of fuel for the host
// if it reaches 0, it does whatever it wants to to handle it.
use fuel;

wasm:opcode:<opcode>(arg0:i32):before / fid == 20 && opidx == <instr_idx> / {
    // Bundle the needed state
    
    // push state onto the stack for the base slice
    whamm_fuel.push_base_i32(arg0);
    // push state onto the stack for the inner loop slice (loop starts at 89)
    whamm_fuel.push_loop_i32(89, arg0);
}

wasm:loop:backedge / fid == 20 && opidx == 89 / {
    // invoke the inner loop that starts on 89 for all backedges
    var cost: i64 = whamm_fuel.invoke_loop(fid, 89);
    fuel.consume(cost);
}

wasm:func:exit / fid == 20 / {
    // invoke the base function slice for function #20
    // This will pop the appropriate values off the stack for this function
    var cost: i64 = whamm_fuel.invoke_base(fid);
    fuel.consume(cost);
}
```


OR, using frame variables:
```
// This is the library that computes the cost for some program slice (per function).
// With this structure we can also put intermediate logic that looks up if we've previously
// invoked the slice with the same inputs (return the previously computed fuel cost).
use whamm_fuel;

// This is the domain-specific implementation of fuel for the host
// if it reaches 0, it does whatever it wants to to handle it.
use fuel;

wasm:opcode:<opcode>(arg0:i32):before / fid == 20 && opidx == <instr_idx> / {
    // Bundle the needed state
    frame var <opcode><instr_idx>_arg0: i32;
    <opcode><instr_idx>_arg0 = arg0;
}

wasm:loop:backedge / fid == 20 && opidx == 89 / {
    frame var <opcode><instr_idx>_arg0: i32;

    // invoke the inner loop that starts on 89 for all backedges
    var cost: i64 = whamm_fuel.invoke_loop_i32(fid, 89, <opcode><instr_idx>_arg0);
    fuel.consume(cost);
}

wasm:func:exit / fid == 20 / {
    // invoke the base function slice for function #20
    // This will pop the appropriate values off the stack for this function
    var cost: i64 = whamm_fuel.invoke_base_i32(fid, <opcode><instr_idx>_arg0);
    fuel.consume(cost);
}
```

OR, using frame variables+dynamic function ID lookup:
```
// This is the library that computes the cost for some program slice (per function).
// With this structure we can also put intermediate logic that looks up if we've previously
// invoked the slice with the same inputs (return the previously computed fuel cost).
use whamm_fuel;

// This is the domain-specific implementation of fuel for the host
// if it reaches 0, it does whatever it wants to to handle it.
use fuel;

wasm:opcode:<opcode>(arg0:i32):before / fid == 20 && opidx == <instr_idx> / {
    // Bundle the needed state
    frame var <opcode><instr_idx>_arg0: i32;
    <opcode><instr_idx>_arg0 = arg0;
}

wasm:loop:backedge / fid == 20 && opidx == 89 / {
    frame var <opcode><instr_idx>_arg0: i32;

    // invoke the inner loop that starts on 89 for all backedges
    var slice_fid: i32 = whamm_fuel.loop_slice_fid(fid, 89);
    var cost: i64 = call(slice_fid, <opcode><instr_idx>_arg0);
    fuel.consume(cost);
}

wasm:func:exit / fid == 20 / {
    // invoke the base function slice for function #20
    // This will pop the appropriate values off the stack for this function
    var cost: i64 = whamm_fuel.invoke_base_i32(fid, <opcode><instr_idx>_arg0);
    fuel.consume(cost);
}
```
