;; Expected: reject unknown ambient import; no WASI filesystem/stdio/network is linked.
(module
  (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1 2)
  (func (export "run") (result i32 i32) i32.const 0 i32.const 0))
