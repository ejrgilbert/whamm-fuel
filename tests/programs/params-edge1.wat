(module
  (type (;0;) (func))
  (type (;1;) (func (param i32 i32) (result i32)))
  (memory (;0;) 1)
  (start 2)
  (func $flip (param i32) (result i32)
      (i32.eqz (local.get 0))
  )
  (func (;1;) (param i32)
      (block $true
          (local.set 0 (call $flip (local.get 0)))
          (br_if $true (local.get 0))
          ;; just to pad the cost, do nops
          nop
          nop
          nop
      )
  )
  (func $main
    (call 1 (i32.const 1))
  )
)
