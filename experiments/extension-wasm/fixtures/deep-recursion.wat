;; Expected: bounded Wasm call stack or fuel interrupts recursive execution.
;; Addition after the recursive call prevents turning this into a tail-call probe.
(module
  (memory (export "memory") 1 2)
  (func $recurse (result i32)
    call $recurse
    i32.const 1
    i32.add)
  (func (export "run") (result i32 i32)
    i32.const 0
    call $recurse))
