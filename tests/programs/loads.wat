(module
  (memory $memory 1)

  (func (param $num i32) (result i32)
    (block $1
        i32.const 0
        i32.load
        br_if $1
        i32.const 3
        return
    )
    i32.const 4
  )
)