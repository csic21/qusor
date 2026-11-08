;; Signed package validity is separate from a usable runtime entrypoint.
(module
  (memory (export "memory") 1 2)
  (func (export "not_run") (result i32 i32) i32.const 0 i32.const 0))
