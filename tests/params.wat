(module
  (type (;0;) (func (param i64) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func))
  (global $var0 (;0;) (mut i32) i32.const 0)
  (global $var1 (;1;) (mut i32) i32.const 0)
  (export "main" (func $main))
  (export "_start" (func $start))
  (func $basic_br (;0;) (type 0) (param i64) (result i32)
    block $eq
      block $neq
        local.get 0
        i64.const 1
        i64.eq
        br_if $eq
        br $neq
      end
      i32.const 0
      return
    end
    i32.const 1
    return
  )
  (func $more_nesting (;1;) (type 1) (param i32) (result i32)
    block $gt
      block $neq
        block $eq
          local.get 0
          i32.const 0
          i32.eq
          br_if $eq
          local.get 0
          i32.const 2
          i32.gt_u
          br_if $gt
          br $neq
        end
        i32.const 1
        return
      end
      i32.const 0
      return
    end
    i32.const 2
    return
  )
  (func $br_table (;2;) (type 1) (param i32) (result i32)
    block $two
      block $one
        block $zero
          local.get 0
          br_table $zero $one $two $two
        end
        i32.const 0
        return
      end
      i32.const 1
      return
    end
    i32.const 2
  )
  (func $if_stmt (;3;) (type 1) (param i32) (result i32)
    local.get 0
    i32.const 1
    i32.eq
    if ;; label = @1
      i32.const 1
      return
    else
      i32.const 0
      return
    end
    i32.const 0
  )
  (func $select_stmt (;4;) (type 1) (param i32) (result i32)
    i32.const 1
    i32.const 0
    local.get 0
    i32.eqz
    select
  )
  (func $main (;5;) (type 2)
    i64.const 0
    call $basic_br
    global.set $var0
    i32.const 0
    call $more_nesting
    global.get $var1
    i32.add
    global.set $var1
    i32.const 1
    call $more_nesting
    global.get $var1
    i32.add
    global.set $var1
    i32.const 1
    call $more_nesting
    global.get $var1
    i32.add
    global.set $var1
    i32.const 3
    call $more_nesting
    global.get $var1
    i32.add
    global.set $var1
    i32.const 0
    call $br_table
    drop
    i32.const 1
    call $br_table
    drop
    i32.const 2
    call $br_table
    drop
    i32.const 0
    call $if_stmt
    drop
    i32.const 0
    call $select_stmt
    drop
  )
  (func $start (;6;) (type 2)
    call $main
  )
)
