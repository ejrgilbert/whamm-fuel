(module
  (type (;0;) (func))
  (memory (;0;) 1)
  (export "main" (func $main))
  (export "_start" (func $start))
  (func $main (;0;) (type 0)
    i32.const 5
    i64.const 1
    i64.store
    i32.const 0
    i32.load
    i32.load offset=8
    drop
  )
  (func $start (;1;) (type 0)
    call $main
  )
)
