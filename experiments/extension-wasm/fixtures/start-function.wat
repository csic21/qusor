;; Expected: runner rejects modules with a start function before any guest execution.
;; The empty start remains an explicit policy violation even though it has no effects.
(module
  (memory (export "memory") 1 2)
  (func $start)
  (start $start)
  (func (export "run") (result i32 i32) i32.const 0 i32.const 0))
