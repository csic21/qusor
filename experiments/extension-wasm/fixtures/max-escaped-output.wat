;; Expected: exactly 4096 NUL bytes are valid UTF-8 output within the raw byte limit.
;; JSON must escape every NUL as six bytes; CLI report limits must allow that expansion.
;; Fresh Wasm linear memory is zero-initialized, so no data segment is required.
(module
  (memory (export "memory") 1 2)
  (func (export "run") (result i32 i32)
    i32.const 0
    i32.const 4096))
