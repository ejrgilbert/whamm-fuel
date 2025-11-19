# `whamm-fuel` # 

This is a repository containing currently in-flux code as we work toward building efficient fuel monitoring using the
[`whamm`](https://github.com/ejrgilbert/whamm) instrumentation framework for WebAssembly.

# TODO List #

- [ ] Finish the testing framework
      - Run the generated modules with input (have expected FUEL amounts)
- [ ] Get this working for APPROXIMATE fuel calculations
- [ ] How to hook up to `whamm` to actually instrument the program at the necessary locations?
      - Create a test framework that actually performs this and tests that it works
- [ ] Extend to support ALL Wasm opcodes (right now it just supports Wasm CORE opcodes)
