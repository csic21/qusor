;; Expected: runner rejects returned output length 4097, above the 4096-byte budget.
(module
  (memory (export "memory") 1 2)
  (func (export "run") (result i32 i32) i32.const 1024 i32.const 4097))
