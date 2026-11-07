;; Expected: broker returns synthetic selection; plugin prefixes those actual bytes.
;; The fixture contains request metadata only, never the synthetic selection text.
(module
  (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1 2)
  (data (i32.const 0) "{\22api_version\22:{\22major\22:0,\22minor\22:1},\22request_id\22:\22probe-request-1\22,\22capability\22:\22selection.read\22,\22resource\22:{\22kind\22:\22selection\22,\22vault_id\22:\22probe-vault\22,\22selection_id\22:\22selection-1\22}}")
  (data (i32.const 1024) "[excerpt] ")
  (func (export "run") (result i32 i32)
    (local $length i32)
    i32.const 0
    i32.const 184
    i32.const 1034
    i32.const 4086
    call $call
    local.set $length
    i32.const 1024
    local.get $length
    i32.const 10
    i32.add))
