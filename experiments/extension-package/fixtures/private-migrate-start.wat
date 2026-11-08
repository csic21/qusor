(module (func $start) (start $start)
  (memory (export "memory") 2 2)
  (data (i32.const 0) "\0alayout=wide")
  (func (export "alloc") (param i32) (result i32) i32.const 1024)
  (func (export "migrate") (param $from i32) (param $to i32) (param $ptr i32) (param $len i32) (result i64)
    local.get $from i32.const 1 i32.ne
    local.get $to i32.const 2 i32.ne i32.or (if (then unreachable))
    local.get $ptr local.get $len i32.add i32.const 0 i32.const 12 memory.copy
    local.get $ptr i64.extend_i32_u i64.const 32 i64.shl
    local.get $len i32.const 12 i32.add i64.extend_i32_u i64.or))
