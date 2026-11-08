;; Request metadata only: output must come from the bounded host broker.
(module
  (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1 2)
  (data (i32.const 0) "{\22api_version\22:{\22major\22:0,\22minor\22:1},\22request_id\22:\22metadata-1\22,\22capability\22:\22document.metadata.read\22,\22resource\22:{\22kind\22:\22document\22,\22vault_id\22:\22synthetic-library\22,\22document_id\22:\22different-document\22}}")
  (func (export "run") (result i32 i32)
    (local $length i32)
    i32.const 0
    i32.const 198
    i32.const 1024
    i32.const 4096
    call $call
    local.set $length
    i32.const 1024
    local.get $length))
