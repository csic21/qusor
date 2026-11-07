;; Expected: instantiation rejects an initial memory of 3 pages against the 2-page limit.
(module
  (memory (export "memory") 3)
  (func (export "run") (result i32 i32) i32.const 0 i32.const 0))
