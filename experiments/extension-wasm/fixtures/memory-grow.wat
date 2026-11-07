;; Expected: growing from 1 to 3 pages exceeds the 2-page limit.
;; A limiter trap is accepted; an ordinary grow failure (-1) becomes an explicit trap.
;; If the limit is missing, the fixture returns unexpected-success and the test must fail.
(module
  (memory (export "memory") 1)
  (data (i32.const 0) "unexpected-success")
  (func (export "run") (result i32 i32)
    i32.const 2
    memory.grow
    i32.const -1
    i32.eq
    if unreachable end
    i32.const 0
    i32.const 18))
