(module
  (type (;0;) (func (param i32) (result i64)))
  (type (;1;) (func (param i32 i32) (result i64)))
  (type (;2;) (func (result i64)))
  (export "exact0" (func 0))
  (export "exact1" (func 1))
  (export "exact2" (func 2))
  (export "exact3" (func 3))
  (export "exact4" (func 4))
  (export "exact5" (func 5))
  (export "exact6" (func 6))
  (func (;0;) (type 0) (param i32) (result i64)
    (local i64)
    block ;; label = @1
      block ;; label = @2
        block ;; label = @3
          local.get 1
          i64.const 6
          i64.add
          local.set 1
          local.get 0
          br_if 1 (;@2;)
          local.get 1
          i64.const 1
          i64.add
          local.set 1
          br 0 (;@3;)
          local.get 1
          i64.const 1
          i64.add
          local.set 1
        end
        local.get 1
        i64.const 2
        i64.add
        local.set 1
        local.get 1
        return
        local.get 1
        i64.const 1
        i64.add
        local.set 1
      end
      local.get 1
      i64.const 2
      i64.add
      local.set 1
      local.get 1
      return
      local.get 1
      i64.const 1
      i64.add
      local.set 1
    end
    local.get 1
  )
  (func (;1;) (type 1) (param i32 i32) (result i64)
    (local i64)
    block ;; label = @1
      block ;; label = @2
        block ;; label = @3
          block ;; label = @4
            local.get 2
            i64.const 7
            i64.add
            local.set 2
            local.get 0
            br_if 0 (;@4;)
            local.get 2
            i64.const 4
            i64.add
            local.set 2
            local.get 1
            br_if 2 (;@2;)
            local.get 2
            i64.const 1
            i64.add
            local.set 2
            br 1 (;@3;)
            local.get 2
            i64.const 1
            i64.add
            local.set 2
          end
          local.get 2
          i64.const 2
          i64.add
          local.set 2
          local.get 2
          return
          local.get 2
          i64.const 1
          i64.add
          local.set 2
        end
        local.get 2
        i64.const 2
        i64.add
        local.set 2
        local.get 2
        return
        local.get 2
        i64.const 1
        i64.add
        local.set 2
      end
      local.get 2
      i64.const 2
      i64.add
      local.set 2
      local.get 2
      return
      local.get 2
      i64.const 1
      i64.add
      local.set 2
    end
    local.get 2
  )
  (func (;2;) (type 0) (param i32) (result i64)
    (local i64)
    block ;; label = @1
      block ;; label = @2
        block ;; label = @3
          block ;; label = @4
            local.get 1
            i64.const 5
            i64.add
            local.set 1
            local.get 0
            br_table 0 (;@4;) 1 (;@3;) 2 (;@2;) 2 (;@2;)
            local.get 1
            i64.const 1
            i64.add
            local.set 1
          end
          local.get 1
          i64.const 2
          i64.add
          local.set 1
          local.get 1
          return
          local.get 1
          i64.const 1
          i64.add
          local.set 1
        end
        local.get 1
        i64.const 2
        i64.add
        local.set 1
        local.get 1
        return
        local.get 1
        i64.const 1
        i64.add
        local.set 1
      end
      local.get 1
      i64.const 2
      i64.add
      local.set 1
    end
    local.get 1
  )
  (func (;3;) (type 0) (param i32) (result i64)
    (local i64)
    block ;; label = @1
      local.get 1
      i64.const 4
      i64.add
      local.set 1
      local.get 0
      if ;; label = @2
        local.get 1
        i64.const 2
        i64.add
        local.set 1
        local.get 1
        return
        local.get 1
        i64.const 1
        i64.add
        local.set 1
      else
        local.get 1
        i64.const 2
        i64.add
        local.set 1
        local.get 1
        return
        local.get 1
        i64.const 1
        i64.add
        local.set 1
      end
      local.get 1
      i64.const 2
      i64.add
      local.set 1
    end
    local.get 1
  )
  (func (;4;) (type 2) (result i64)
    (local i64)
    block ;; label = @1
      local.get 0
      i64.const 6
      i64.add
      local.set 0
    end
    local.get 0
  )
  (func (;5;) (type 2) (result i64)
    (local i64)
    block ;; label = @1
      local.get 0
      i64.const 41
      i64.add
      local.set 0
    end
    local.get 0
  )
  (func (;6;) (type 2) (result i64)
    (local i64)
    block ;; label = @1
      local.get 0
      i64.const 2
      i64.add
      local.set 0
    end
    local.get 0
  )
)
