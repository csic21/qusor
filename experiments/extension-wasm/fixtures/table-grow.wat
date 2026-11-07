;; Expected: growing from 1 to 17 table entries exceeds the 16-entry limit.
;; A limiter trap is accepted; an ordinary grow failure (-1) becomes an explicit trap.
(module
  (memory (export "memory") 1 2)
  (table 1 funcref)
  (data (i32.const 0) "unexpected-success")
  (func (export "run") (result i32 i32)
    ref.null func
    i32.const 16
    table.grow
    i32.const -1
    i32.eq
    if unreachable end
    i32.const 0
    i32.const 18))
