;; Ordinary text provider A: fresh-VM streaming, max four calls; no injected text.
(module
 (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
 (memory (export "memory") 2 2)
 (data(i32.const 0)"\22\72\65\73\6f\75\72\63\65\22")
 (data(i32.const 10)"\22\6d\6f\75\6e\74\5f\69\64\22")
 (data(i32.const 20)"\22\73\65\71\75\65\6e\63\65\22")
 (data(i32.const 30)"\22\65\78\70\65\63\74\65\64\5f\72\65\76\69\73\69\6f\6e\22")
 (data(i32.const 49)"\22\6f\75\74\63\6f\6d\65\22")
 (data(i32.const 58)"\22\61\63\74\69\6f\6e\22")
 (data(i32.const 66)"\22\6b\69\6e\64\22")
 (data(i32.const 72)"\22\69\6e\70\75\74\5f\63\68\61\6e\67\65\64\22")
 (data(i32.const 87)"\22\74\6f\74\61\6c\5f\62\79\74\65\73\22")
 (data(i32.const 100)"\22\6d\65\74\61\64\61\74\61\22")
 (data(i32.const 110)"\22\73\74\61\74\65\22")
 (data(i32.const 117)"\22\61\63\74\69\76\65\22")
 (data(i32.const 125)"\22\63\68\65\63\6b\70\6f\69\6e\74\5f\72\65\76\69\73\69\6f\6e\22")
 (data(i32.const 146)"\22\6c\61\73\74\5f\65\66\66\65\63\74\22")
 (data(i32.const 159)"\6e\75\6c\6c")
 (data(i32.const 163)"\22\73\74\61\72\74\65\64\22")
 (data(i32.const 172)"\22\63\68\75\6e\6b\5f\61\63\63\65\70\74\65\64\22")
 (data(i32.const 188)"\22\6e\65\78\74\5f\6f\66\66\73\65\74\22")
 (data(i32.const 201)"\30")
 (data(i32.const 202)"\22\74\65\78\74\22")
 (data(i32.const 208)"\7b\22\61\70\69\5f\76\65\72\73\69\6f\6e\22\3a\7b\22\6d\61\6a\6f\72\22\3a\30\2c\22\6d\69\6e\6f\72\22\3a\33\7d\2c\22\63\6f\6e\74\65\78\74\5f\70\72\6f\74\6f\63\6f\6c\22\3a\31\2c\22\72\65\71\75\65\73\74\5f\69\64\22\3a\22\63\6f\6e\74\65\78\74\22\2c\22\6f\70\65\72\61\74\69\6f\6e\22\3a\7b\22\6b\69\6e\64\22\3a\22\72\65\61\64\5f\63\6f\6e\74\65\78\74\22\7d\7d")
 (data(i32.const 325)"\7b\22\61\70\69\5f\76\65\72\73\69\6f\6e\22\3a\7b\22\6d\61\6a\6f\72\22\3a\30\2c\22\6d\69\6e\6f\72\22\3a\33\7d\2c\22\63\6f\6e\74\69\6e\75\61\74\69\6f\6e\5f\70\72\6f\74\6f\63\6f\6c\22\3a\31\2c\22\72\65\71\75\65\73\74\5f\69\64\22\3a\22\61\64\76\61\6e\63\65\22\2c\22\72\65\73\6f\75\72\63\65\22\3a")
 (data(i32.const 422)"\2c\22\6d\6f\75\6e\74\5f\69\64\22\3a")
 (data(i32.const 434)"\2c\22\73\65\71\75\65\6e\63\65\22\3a")
 (data(i32.const 446)"\2c\22\62\61\73\65\5f\72\65\76\69\73\69\6f\6e\22\3a")
 (data(i32.const 463)"\2c\22\6f\70\65\72\61\74\69\6f\6e\22\3a\7b\22\6b\69\6e\64\22\3a\22\72\65\61\64\5f\6d\65\74\61\64\61\74\61\22\7d\7d")
 (data(i32.const 501)"\2c\22\6f\70\65\72\61\74\69\6f\6e\22\3a\7b\22\6b\69\6e\64\22\3a\22\61\64\76\61\6e\63\65\5f\65\76\65\6e\74\22\2c\22\65\78\70\65\63\74\65\64\5f\63\68\65\63\6b\70\6f\69\6e\74\5f\72\65\76\69\73\69\6f\6e\22\3a")
 (data(i32.const 569)"\2c\22\63\68\65\63\6b\70\6f\69\6e\74\5f\62\61\73\65\36\34\22\3a\22\22\2c\22\63\6f\6d\70\6c\65\74\65\22\3a\66\61\6c\73\65\2c\22\65\66\66\65\63\74\22\3a\7b\22\6b\69\6e\64\22\3a\22\62\65\67\69\6e\5f\72\65\70\6c\61\63\65\22\2c\22\65\78\70\65\63\74\65\64\5f\72\65\76\69\73\69\6f\6e\22\3a")
 (data(i32.const 663)"\2c\22\74\6f\74\61\6c\5f\62\79\74\65\73\22\3a")
 (data(i32.const 678)"\7d\7d\7d")
 (data(i32.const 681)"\2c\22\63\68\65\63\6b\70\6f\69\6e\74\5f\62\61\73\65\36\34\22\3a\22\22\2c\22\63\6f\6d\70\6c\65\74\65\22\3a\74\72\75\65\2c\22\65\66\66\65\63\74\22\3a\7b\22\6b\69\6e\64\22\3a\22\63\6f\6d\6d\69\74\5f\72\65\70\6c\61\63\65\22\7d\7d\7d")
 (data(i32.const 757)"\2c\22\63\68\65\63\6b\70\6f\69\6e\74\5f\62\61\73\65\36\34\22\3a\22\22\2c\22\63\6f\6d\70\6c\65\74\65\22\3a\66\61\6c\73\65\2c\22\65\66\66\65\63\74\22\3a\7b\22\6b\69\6e\64\22\3a\22\61\70\70\65\6e\64\5f\63\68\75\6e\6b\22\2c\22\6f\66\66\73\65\74\22\3a")
 (data(i32.const 839)"\2c\22\74\65\78\74\22\3a")
 (data(i32.const 847)"\7b\22\61\70\69\5f\76\65\72\73\69\6f\6e\22\3a\7b\22\6d\61\6a\6f\72\22\3a\30\2c\22\6d\69\6e\6f\72\22\3a\33\7d\2c\22\63\6f\6e\74\65\78\74\5f\70\72\6f\74\6f\63\6f\6c\22\3a\31\2c\22\72\65\71\75\65\73\74\5f\69\64\22\3a\22\69\6e\70\75\74\22\2c\22\6f\70\65\72\61\74\69\6f\6e\22\3a\7b\22\6b\69\6e\64\22\3a\22\72\65\61\64\5f\69\6e\70\75\74\5f\63\68\75\6e\6b\22\2c\22\73\65\71\75\65\6e\63\65\22\3a")
 (data(i32.const 976)"\2c\22\65\78\70\65\63\74\65\64\5f\72\65\76\69\73\69\6f\6e\22\3a")
 (data(i32.const 997)"\2c\22\6f\66\66\73\65\74\22\3a")
 (data(i32.const 1007)"\2c\22\6d\61\78\5f\62\79\74\65\73\22\3a\35\31\32\7d\7d")
 (global $out (mut i32) (i32.const 32768))
 (global $cp (mut i32) (i32.const 8192)) (global $cn (mut i32) (i32.const 0))
 (func $eq (param $p i32)(param $n i32)(param $q i32)(param $m i32)(result i32)(local $i i32)
  local.get $n local.get $m i32.ne if i32.const 0 return end
  block $done loop $loop local.get $i local.get $n i32.ge_u br_if $done
   local.get $p local.get $i i32.add i32.load8_u local.get $q local.get $i i32.add i32.load8_u i32.ne if i32.const 0 return end
   local.get $i i32.const 1 i32.add local.set $i br $loop end end i32.const 1)
 (func $ws(param $p i32)(param $end i32)(result i32)
  block $done loop $loop local.get $p local.get $end i32.ge_u br_if $done
   local.get $p i32.load8_u i32.const 32 i32.gt_u br_if $done
   local.get $p i32.const 1 i32.add local.set $p br $loop end end local.get $p)
 ;; Scan one JSON value; callers only pass bounded broker-validated JSON.
 (func $endvalue(param $p i32)(param $end i32)(result i32)(local $depth i32)(local $quoted i32)(local $c i32)
  loop $loop
   local.get $p local.get $end i32.ge_u if unreachable end
   local.get $p i32.load8_u local.set $c
   local.get $quoted if
    local.get $c i32.const 92 i32.eq if local.get $p i32.const 2 i32.add local.set $p br $loop end
    local.get $c i32.const 34 i32.eq if
     i32.const 0 local.set $quoted
     local.get $depth i32.eqz if local.get $p i32.const 1 i32.add return end
    end
   else
    local.get $c i32.const 34 i32.eq if i32.const 1 local.set $quoted else
     local.get $c i32.const 123 i32.eq local.get $c i32.const 91 i32.eq i32.or if
      local.get $depth i32.const 1 i32.add local.set $depth
     else
      local.get $c i32.const 125 i32.eq local.get $c i32.const 93 i32.eq i32.or if
       local.get $depth i32.eqz if local.get $p return end
       local.get $depth i32.const 1 i32.sub local.tee $depth i32.eqz if local.get $p i32.const 1 i32.add return end
      else
       local.get $depth i32.eqz if
        local.get $c i32.const 44 i32.eq local.get $c i32.const 32 i32.le_u i32.or if local.get $p return end
       end
      end
     end
    end
   end
   local.get $p i32.const 1 i32.add local.set $p br $loop
  end unreachable)
 (func $field(param $p i32)(param $n i32)(param $key i32)(param $kn i32)(result i32 i32)
  (local $end i32)(local $kp i32)(local $ke i32)(local $ve i32)(local $hit i32)
  local.get $p local.get $n i32.add local.set $end
  local.get $p i32.load8_u i32.const 123 i32.ne if unreachable end
  local.get $p i32.const 1 i32.add local.set $p
  loop $loop
   local.get $p local.get $end call $ws local.set $p
   local.get $p local.set $kp
   local.get $p local.get $end call $endvalue local.set $ke
   local.get $kp local.get $ke local.get $kp i32.sub local.get $key local.get $kn call $eq local.set $hit
   local.get $ke local.get $end call $ws local.tee $p i32.load8_u i32.const 58 i32.ne if unreachable end
   local.get $p i32.const 1 i32.add local.get $end call $ws local.set $p
   local.get $p local.get $end call $endvalue local.set $ve
   local.get $hit if local.get $p local.get $ve local.get $p i32.sub return end
   local.get $ve local.get $end call $ws local.tee $p i32.load8_u i32.const 44 i32.ne if unreachable end
   local.get $p i32.const 1 i32.add local.set $p br $loop
  end unreachable)
 (func $emit(param $p i32)(param $n i32)
  global.get $out local.get $n i32.add i32.const 65536 i32.gt_u if unreachable end
  global.get $out local.get $p local.get $n memory.copy
  global.get $out local.get $n i32.add global.set $out)
 (func $cf(param $key i32)(param $kn i32) global.get $cp global.get $cn local.get $key local.get $kn call $field call $emit)
 (func $number(param $p i32)(param $n i32)(result i32)(local $i i32)(local $v i32)(local $c i32)
  local.get $n i32.const 7 i32.gt_u if unreachable end
  block $done loop $loop local.get $i local.get $n i32.ge_u br_if $done
   local.get $p local.get $i i32.add i32.load8_u i32.const 48 i32.sub local.tee $c i32.const 9 i32.gt_u if unreachable end
   local.get $v i32.const 10 i32.mul local.get $c i32.add local.set $v
   local.get $i i32.const 1 i32.add local.set $i br $loop end end local.get $v)
 (func $begin i32.const 32768 global.set $out)
 (func $rpc(param $reply i32)(result i32)
  i32.const 32768 global.get $out i32.const 32768 i32.sub local.get $reply i32.const 4096 call $call
  local.tee $reply i32.const 0 i32.lt_s if unreachable end local.get $reply)
 (func $identity
  i32.const 325 i32.const 97 call $emit
  i32.const 0 i32.const 10 call $cf i32.const 422 i32.const 12 call $emit i32.const 10 i32.const 10 call $cf
  i32.const 434 i32.const 12 call $emit i32.const 20 i32.const 10 call $cf
  i32.const 446 i32.const 17 call $emit i32.const 30 i32.const 19 call $cf)
 (func(export "run")(result i32 i32)
  (local $mp i32)(local $mn i32)(local $op i32)(local $on i32)(local $ap i32)(local $an i32)
  (local $tp i32)(local $tn i32)(local $rp i32)(local $rn i32)(local $ep i32)(local $en i32)
  (local $kp i32)(local $kn i32)(local $offsetp i32)(local $offsetn i32)(local $n i32)
  call $begin i32.const 208 i32.const 117 call $emit i32.const 8192 call $rpc global.set $cn
  i32.const 8192 global.get $cn i32.const 49 i32.const 9 call $field local.set $on local.set $op
  local.get $op local.get $on i32.const 58 i32.const 8 call $field local.set $an local.set $ap
  local.get $ap local.get $an i32.const 66 i32.const 6 call $field i32.const 72 i32.const 15 call $eq i32.eqz if unreachable end
  local.get $ap local.get $an i32.const 87 i32.const 13 call $field local.set $tn local.set $tp
  call $begin call $identity i32.const 463 i32.const 38 call $emit i32.const 12288 call $rpc local.set $n
  i32.const 12288 local.get $n i32.const 49 i32.const 9 call $field local.set $on local.set $op
  local.get $op local.get $on i32.const 100 i32.const 10 call $field local.set $mn local.set $mp
  local.get $mp local.get $mn i32.const 110 i32.const 7 call $field i32.const 117 i32.const 8 call $eq i32.eqz if i32.const 12288 local.get $n return end
  local.get $mp local.get $mn i32.const 125 i32.const 21 call $field local.set $rn local.set $rp
  local.get $mp local.get $mn i32.const 146 i32.const 13 call $field local.set $en local.set $ep
  ;; Start the event-owned upload once. All subsequent fresh VMs use last_effect.
  local.get $ep local.get $en i32.const 159 i32.const 4 call $eq if
   call $begin call $identity i32.const 501 i32.const 68 call $emit local.get $rp local.get $rn call $emit
   i32.const 569 i32.const 94 call $emit i32.const 30 i32.const 19 call $cf i32.const 663 i32.const 15 call $emit local.get $tp local.get $tn call $emit i32.const 678 i32.const 3 call $emit
  else
   local.get $ep local.get $en i32.const 66 i32.const 6 call $field local.set $kn local.set $kp
   local.get $kp local.get $kn i32.const 163 i32.const 9 call $eq if
    i32.const 201 i32.const 1 local.set $offsetn local.set $offsetp
   else
    local.get $kp local.get $kn i32.const 172 i32.const 16 call $eq i32.eqz if unreachable end
    local.get $ep local.get $en i32.const 188 i32.const 13 call $field local.set $offsetn local.set $offsetp
   end
   local.get $offsetp local.get $offsetn call $number local.get $tp local.get $tn call $number i32.eq if
    call $begin call $identity i32.const 501 i32.const 68 call $emit local.get $rp local.get $rn call $emit i32.const 681 i32.const 76 call $emit
   else
    call $begin i32.const 847 i32.const 129 call $emit i32.const 20 i32.const 10 call $cf i32.const 976 i32.const 21 call $emit i32.const 30 i32.const 19 call $cf i32.const 997 i32.const 10 call $emit local.get $offsetp local.get $offsetn call $emit i32.const 1007 i32.const 18 call $emit
    i32.const 16384 call $rpc local.set $n
    i32.const 16384 local.get $n i32.const 49 i32.const 9 call $field local.set $on local.set $op
    local.get $op local.get $on i32.const 202 i32.const 6 call $field local.set $an local.set $ap
    call $begin call $identity i32.const 501 i32.const 68 call $emit local.get $rp local.get $rn call $emit i32.const 757 i32.const 82 call $emit local.get $offsetp local.get $offsetn call $emit i32.const 839 i32.const 8 call $emit local.get $ap local.get $an call $emit i32.const 678 i32.const 3 call $emit
   end
  end
  i32.const 20480 call $rpc local.set $n i32.const 20480 local.get $n)
)
