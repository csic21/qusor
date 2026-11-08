(module
  (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 2 2)
  (func (export "alloc") (param i32) (result i32) i32.const 1024)
  (func (export "run") (result i32 i32) i32.const 0 i32.const 0)
  (func (export "qusor_abi_version") (result i32) i32.const 1)
  (func (export "qusor_state_check") (param i32 i32 i32) (result i32)
    i32.const 0 i32.const 0 i32.const 0 i32.const 0 call $call))
