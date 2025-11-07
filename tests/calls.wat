(module
  (func $cond (result i32)
      i32.const 1
  )

  (func (param $num i32) (result i32)
    (block $1
        call $cond
        br_if $1
        i32.const 3
        return
    )
    i32.const 4
  )
)