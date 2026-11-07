;; Expected: request length above 4096 is rejected before JSON parsing or allocation.
(module
  (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1 2)
  (data (i32.const 0) "{\22api_version\22:{\22major\22:0,\22minor\22:1},\22request_id\22:\22probe-request-1\22,\22capability\22:\22selection.read\22,\22resource\22:{\22kind\22:\22selection\22,\22vault_id\22:\22probe-vault\22,\22selection_id\22:\22selection-1\22}}")
  (func (export "run") (result i32 i32)
    i32.const 0
    i32.const 4097
    i32.const 1024
    i32.const 4096
    call $call
    drop
    i32.const 0
    i32.const 0))
