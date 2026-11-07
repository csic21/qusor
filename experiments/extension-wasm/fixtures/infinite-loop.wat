;; Expected: fuel budget terminates execution; no broker call is needed.
(module
  (memory (export "memory") 1 2)
  (func (export "run") (result i32 i32)
    (loop $forever (br $forever))
    i32.const 0
    i32.const 0))
