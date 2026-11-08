(module
  (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 2 2)
  (data (i32.const 0) "\0alayout=wide")
  (func (export "alloc") (param i32) (result i32) i32.const 1024)
  (func (export "run") (result i32 i32) i32.const 0 i32.const 0)
  (func (export "qusor_abi_version") (result i32) i32.const 1)
  (func (export "qusor_state_check") (param $schema i32) (param $ptr i32) (param $len i32) (result i32)
    (local $i i32)
    local.get $schema i32.const 2 i32.ne (if (then i32.const 1 return))
    local.get $len i32.const 12 i32.lt_u (if (then i32.const 1 return))
    (loop $check
      local.get $ptr local.get $len i32.add i32.const 12 i32.sub local.get $i i32.add i32.load8_u
      local.get $i i32.load8_u i32.ne (if (then i32.const 1 return))
      local.get $i i32.const 1 i32.add local.tee $i i32.const 12 i32.lt_u br_if $check)
    i32.const 0))
