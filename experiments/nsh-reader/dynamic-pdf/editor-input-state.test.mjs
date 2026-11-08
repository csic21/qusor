import assert from 'node:assert/strict';
import {editorInput,localInput,composition,deliver,applied,rejected,unconfirmed,deliveryReply,retryDelivery,inputCanSave} from './editor-input-state.ts';

let state=localInput(editorInput('first',2,''),'first','中');
let delivery=deliver(state);state=delivery.state;
state=localInput(state,'first','中文🙂\0');
assert.equal(deliver(state),null,'only one actual delivery may run');
state=applied(state,delivery.command,3);
assert.equal(state.localText,'中文🙂\0','older completion cannot overwrite fresh input');
assert.equal(state.appliedText,'中');assert.equal(inputCanSave(state),false);
const latest=deliver(state);assert.equal(latest.command.baseRevision,3);
assert.equal(latest.command.text,'中文🙂\0');
state=applied(latest.state,latest.command,4);assert.equal(inputCanSave(state),true);
assert.equal(applied(state,delivery.command,3),state,'late duplicate reply is ignored');

let composing=composition(localInput(editorInput('ime',1,''),'ime','组合'),'ime',true);
assert.equal(deliver(composing),null);assert.equal(inputCanSave(composing),false);
composing=composition(composing,'ime',false);assert.ok(deliver(composing));

let unknown=deliver(localInput(editorInput('unknown',7,''),'unknown','first payload'));
let preserved=unconfirmed(unknown.state,unknown.command);
preserved=localInput(preserved,'unknown','newer local payload');
assert.equal(deliver(preserved),null);assert.equal(inputCanSave(preserved),false);
const retry=retryDelivery(preserved);
assert.equal(retry.command,unknown.command,'unknown result retries the original payload');
preserved=applied(retry.state,retry.command,8);
assert.equal(preserved.localText,'newer local payload');
assert.equal(deliver(preserved).command.baseRevision,8);

const replacement=editorInput('replacement',1,'');
assert.equal(applied(replacement,unknown.command,8),replacement);
assert.equal(localInput(replacement,'unknown','old callback'),replacement);
const mismatch=applied(unknown.state,unknown.command,99);
assert.equal(mismatch.uncertain,true);assert.equal(mismatch.inflight,unknown.command);
assert.equal(mismatch.localText,'first payload');
const invalid=deliver(localInput(editorInput('budget',1,''),'budget','too large'));
let refused=rejected(invalid.state,invalid.command);
assert.equal(refused.localText,'too large');assert.equal(refused.revision,1);
assert.equal(deliver(refused),null,'known rejection must not loop on identical input');
refused=localInput(refused,'budget','corrected');
assert.equal(deliver(refused).command.baseRevision,1);
assert.equal(rejected(replacement,invalid.command),replacement);
for(const reply of [null,{}, {ok:true,value:{binding:'other',revision:8,applied:true}},
  {ok:false,code:'editor_busy',effect_unknown:true},
  {ok:false,code:'input_rejected',effect_unknown:true}]) {
  const retained=deliveryReply(unknown.state,unknown.command,reply);
  assert.equal(retained.inflight,unknown.command);assert.equal(retained.uncertain,true);
}
assert.equal(deliveryReply(unknown.state,unknown.command,{ok:false,code:'input_rejected',effect_unknown:false}).inflight,undefined);
assert.equal(deliveryReply(unknown.state,unknown.command,{ok:true,value:{binding:'unknown',revision:8,applied:true}}).revision,8);
console.log('editor input: serial latest snapshot, IME gate, exact unknown retry, stale callback and save gate passed');
