(module
  (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1 2)
  (data (i32.const 0) "{\"notes_protocol\":1,\"api_version\":{\"major\":0,\"minor\":4},\"request_id\":\"notes-page\",\"operation\":{\"kind\":\"page\",\"cursor\":null,\"limit\":6}}")
  (func (export "run") (result i32 i32)
    (local $n i32)
    (local.set $n
      (call $call (i32.const 0) (i32.const 134) (i32.const 4096) (i32.const 4096)))
    (i32.const 4096)
    (local.get $n)))
