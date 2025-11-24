(module
  (type (;0;) (func))
  (memory (;0;) 1)
  (export "_start" (func $main))
  (func $main (;0;) (type 0)
    i32.const 10
    i64.const 1
    i64.store
    i32.const 0
    i32.load
    i32.load offset=3
    drop
  )
  (data (;0;) (i32.const 0) "\01\00\00\00\02\00\00\00")
)
