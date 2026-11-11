(module
  (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1 2)
  (data (i32.const 0) "{\"vocab_protocol\":1,\"api_version\":{\"major\":0,\"minor\":5},\"request_id\":\"vocab-decks\",\"operation\":{\"kind\":\"decks\"}}")
  (data (i32.const 256) "{\"vocab_protocol\":1,\"api_version\":{\"major\":0,\"minor\":5},\"request_id\":\"vocab-queue\",\"operation\":{\"kind\":\"queue\",\"wordbook_id\":1,\"limit\":3}}")
  (data (i32.const 512) "{\"vocab_protocol\":1,\"api_version\":{\"major\":0,\"minor\":5},\"request_id\":\"vocab-review-1\",\"operation\":{\"kind\":\"review\",\"card_id\":1,\"rating\":\"good\"}}")
  (func (export "run") (result i32 i32)
    (local $n i32)
    (drop (call $call (i32.const 0) (i32.const 112) (i32.const 4096) (i32.const 4096)))
    (drop (call $call (i32.const 256) (i32.const 138) (i32.const 4096) (i32.const 4096)))
    (local.set $n
      (call $call (i32.const 512) (i32.const 144) (i32.const 4096) (i32.const 4096)))
    (i32.const 4096)
    (local.get $n)))
