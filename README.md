# `whamm-fuel` # 

This is a repository containing currently in-flux code as we work toward building efficient fuel monitoring using the
[`whamm`](https://github.com/ejrgilbert/whamm) instrumentation framework for WebAssembly.

# TODO List #

- [ ] Fix the `loop` handling
- [ ] Add more tests! (maybe just assert they're valid for now?)
- [ ] Get working e2e (with whamm instrumentation)
      - Should codegen a whamm script (do we need new language features?)
      - Should it return the fuel cost? Decrement handled in `whamm` (gets reported automatically)
- [ ] Get this working for APPROXIMATE fuel calculations
- [ ] `async` analysis (how to buffer events? likely similar to buffering of state in `whamm` invocation above)
- [ ] offline analysis (after finishes executing)
- [ ] How to hook up to `whamm` to actually instrument the program at the necessary locations?
      - Create a test framework that actually performs this and tests that it works
- [ ] Extend to support ALL Wasm opcodes (right now it just supports Wasm CORE opcodes)

## Infinite loop in globals test ##
Change the way I handle `loops`:
- If I see a loop, this needs to be treated similarly to a function call
- The loop itself handles calculating its own cost
- On generating a slice for it:
  - Change the `loop` opcode to a `block` (keeps the generated code valid!)
  - Generate the slice of its BODY
  - This should be a standalone function that is called, passes relevant state to it
- For instrumentation, inject call to this loop with its bundled state for all backedges!
