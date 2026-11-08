.functype work () -> (i64)
.globl run
.functype run () -> (i32, i32)
run:
 .functype run () -> (i32, i32)
 .local i64
 call work
 local.set 0
 local.get 0
 i32.wrap_i64
 local.get 0
 i64.const 32
 i64.shr_u
 i32.wrap_i64
 end_function
