# `whamm-fuel` # 

This is a repository containing currently in-flux code as we work toward building efficient fuel monitoring using the
[`whamm`](https://github.com/ejrgilbert/whamm) instrumentation framework for WebAssembly.

# TODO List #

TO FIX:
- The way I handle if/else is wrong (see func 3 in params.wasm)
- Infinite loop in globals test

- [ ] Finish the testing framework
      - Refactor code to be able to check the output of the program too!
      - Write the expected fuel amounts!!
      - Get all test cases passing!
      - Add many, many more! (maybe just assert they're valid for now?)
- [ ] Get this working for APPROXIMATE fuel calculations
- [ ] How to hook up to `whamm` to actually instrument the program at the necessary locations?
      - Create a test framework that actually performs this and tests that it works
- [ ] Extend to support ALL Wasm opcodes (right now it just supports Wasm CORE opcodes)
