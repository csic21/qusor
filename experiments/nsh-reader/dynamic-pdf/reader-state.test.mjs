import assert from 'node:assert/strict';
import { selectedDraft, editNote, submit, settle, frameKey, pinned, sourceUnavailable, submitEditor, editorEventMatches, chunkEnd, activeInputEvent, deferInputFocus, editorRows, editorRowKey, canDiscardPreparation, canDiscardDraft } from './reader-state.ts';
const a={document_id:1,report_digest:'a',handle:'first',generation:2,frame_id:'frame-a',title:'First source',page:2};
const b={...a,document_id:2,report_digest:'b',handle:'second',frame_id:'frame-b'};
let draft=selectedDraft(a,'candidate-a','exact source text');
draft={...draft,phase:'editing'};
draft=editNote(draft,'candidate-a',frameKey(a),'personal thought');
assert.equal(pinned(draft),true);
assert.equal(editNote(draft,'candidate-b',frameKey(b),'late input'),draft);
const pending=submit(draft);assert.equal(pending.submission.note,'personal thought');
assert.equal(editNote(pending,'candidate-a',frameKey(a),'changed while saving'),pending);
const unknown=settle(pending,'candidate-a',frameKey(a));
assert.equal(unknown.phase,'uncertain');
const retry=submit(unknown);assert.equal(retry.submission,pending.submission);
assert.equal(retry.submission.note,'personal thought');
assert.equal(settle(retry,'candidate-a',frameKey(b),{}),retry);
const saved=settle(retry,'candidate-a',frameKey(a),{capture_id:1});
assert.equal(saved.phase,'saved');assert.equal(submit(saved),null);
assert.equal(editNote(saved,'candidate-a',frameKey(a),'cannot reuse committed candidate'),saved);
const newSelection=selectedDraft(b,'candidate-b','new exact source');
assert.equal(newSelection.note,'');assert.notEqual(newSelection.binding,saved.binding);
assert.equal(settle(newSelection,'candidate-a',frameKey(a),{capture_id:1}),newSelection);
console.log('reader state: source-bound input, immutable retry, stale result rejection, consumed candidate passed');

const failedSourceDuringSave=sourceUnavailable(pending);
assert.equal(failedSourceDuringSave.phase,'saving');assert.equal(failedSourceDuringSave.submission,pending.submission);
const receiptAfterSourceFailure=settle(failedSourceDuringSave,'candidate-a',frameKey(a),{capture_id:2});
assert.equal(receiptAfterSourceFailure.phase,'saved');assert.equal(receiptAfterSourceFailure.sourceUnavailable,true);
const unknownSource=settle(failedSourceDuringSave,'candidate-a',frameKey(a));
assert.equal(unknownSource.phase,'uncertain');assert.equal(unknownSource.submission,pending.submission);assert.equal(submit(unknownSource),null);
const detached=sourceUnavailable(draft);assert.equal(detached.note,draft.note);assert.equal(detached.quote,draft.quote);assert.equal(detached.binding,frameKey(a));assert.equal(detached.phase,'detached');assert.equal(submit(detached),null);
assert.equal(editNote(newSelection,'candidate-a',frameKey(a),'late old field'),newSelection);
console.log('late source failure preserves pending save receipt/retry identity; detached draft retains original source');

const nativeDraft={...selectedDraft(a,'native-candidate','source'),phase:'editing',note:'latest local'};
const nativePending=submitEditor(nativeDraft,'opaque-editor',7);
assert.deepEqual(nativePending.submission.editor,{binding:'opaque-editor',revision:7});
const unknownNative=settle(nativePending,nativeDraft.id,nativeDraft.binding);
const retriedNative=submitEditor(unknownNative,'later-binding',99);
assert.equal(retriedNative.submission,nativePending.submission,'unknown save retries immutable native binding and revision');
assert.equal(retriedNative.submission.note,'latest local');
assert.equal(settle(retriedNative,'old-candidate',nativeDraft.binding,{saved:true}),retriedNative,'old receipt cannot settle successor');
console.log('native save: immutable binding/revision retained through unknown retry and stale receipt');

const unavailableNative=sourceUnavailable(unknownNative);
const recoverUnavailable=submitEditor(unavailableNative,'wrong-new-binding',90);
assert.equal(recoverUnavailable.phase,'saving');
assert.equal(recoverUnavailable.submission,nativePending.submission,'source loss retains frozen native operation');
assert.equal(submitEditor({...nativeDraft,sourceUnavailable:true},'editor',7),null,'source loss cannot freeze a new save');
assert.equal(editorEventMatches(nativeDraft,'native-new',nativeDraft.id,nativeDraft.binding,'native-old'),false,'old native node cannot feed replacement editor');
assert.equal(editorEventMatches(nativeDraft,'native-new','old-candidate',nativeDraft.binding,'native-new'),false);
assert.equal(editorEventMatches({...nativeDraft,phase:'saving'},'native-new',nativeDraft.id,nativeDraft.binding,'native-new'),false);
assert.equal(editorEventMatches(nativeDraft,'native-new',nativeDraft.id,nativeDraft.binding,'native-new'),true);
const unicodeChunks=['中'.repeat(170),'🙂'.repeat(128),'é'.repeat(240)];
let byteOffset=0;const unicodeTotal=new TextEncoder().encode(unicodeChunks.join('')).length;
for(const text of unicodeChunks)byteOffset=chunkEnd(byteOffset,text,unicodeTotal);
assert.equal(byteOffset,unicodeTotal);assert.ok(unicodeTotal>512);
assert.throws(()=>chunkEnd(0,'🙂'.repeat(129),unicodeTotal));
console.log('native recovery: source unavailable frozen retry, node identity races and >512 Unicode byte offsets passed');

const oldNode={},newNode={};
assert.equal(activeInputEvent(newNode,oldNode,true)&&editorEventMatches(nativeDraft,'same-native',nativeDraft.id,nativeDraft.binding,'same-native'),false,'same identities cannot revive an old Input instance');
assert.equal(activeInputEvent(newNode,newNode,true),true);
assert.equal(activeInputEvent(newNode,newNode,false),false,'hidden auxiliary rejects late input');
assert.equal(activeInputEvent(null,oldNode,true),false);
console.log('opaque Input instance reference and hidden-panel event rejection passed');

const focusQueue=[];let focusCount=0,focusValid=true;
const scheduleFocus=(work,delay)=>focusQueue.push({work,delay});
deferInputFocus(scheduleFocus,()=>focusValid,()=>focusCount++);
assert.equal(focusCount,0,'ref does not synchronously consume focus');
assert.equal(focusQueue.shift().delay,0);
// Fresh schedule: run first turn, then invalidate before native mount delay finishes.
deferInputFocus(scheduleFocus,()=>focusValid,()=>focusCount++);
focusQueue.shift().work();assert.equal(focusQueue[0].delay,32);
focusValid=false;focusQueue.shift().work();assert.equal(focusCount,0,'stale or hidden instance cannot steal focus');
focusValid=true;deferInputFocus(scheduleFocus,()=>focusValid,()=>focusCount++);
focusQueue.shift().work();assert.equal(focusCount,0);focusQueue.shift().work();assert.equal(focusCount,1);
console.log('deferred focus: mount turn, readiness recheck and stale timer rejection passed');

const preparingRow=editorRows(nativeDraft,'native-a',false)[0];
const readyRow=editorRows(nativeDraft,'native-a',true)[0];
assert.notEqual(editorRowKey(preparingRow),editorRowKey(readyRow),'readiness changes the tracked row key and creates editable ref');
assert.equal(editorRowKey(readyRow),editorRowKey(editorRows({...nativeDraft,note:'new typing'},'native-a',true)[0]),'typing updates same Input instance');
assert.notEqual(editorRowKey(readyRow),editorRowKey(editorRows(nativeDraft,'native-b',true)[0]));
assert.deepEqual(editorRows(null,'native-a',true),[]);
console.log('tracked Input rows: readiness/binding remount, typing preserves instance passed');

assert.equal(canDiscardPreparation('known-binding',false,false),true,'failed partial read can discard its acquired binding');
assert.equal(canDiscardPreparation('',false,false),false,'unknown begin without binding must retry discovery');
assert.equal(canDiscardPreparation('known-binding',false,true),false,'preparing call remains protected');
assert.equal(canDiscardPreparation('known-binding',true,false),false,'ready editor uses ordinary discard');
console.log('preparation recovery: acquired binding permits explicit discard, unknown/inflight begin does not');

assert.equal(canDiscardDraft(nativeDraft,'binding',false,false,false,false),true);
assert.equal(canDiscardDraft({...nativeDraft,phase:'detached'},'binding',false,false,false,false),true,'partial-read detached binding remains discardable');
for(const phase of ['saving','uncertain'])assert.equal(canDiscardDraft({...nativeDraft,phase},'binding',false,false,false,false),false);
assert.equal(canDiscardDraft(nativeDraft,'',false,false,false,false),false);
for(const flags of [[true,false,false,false],[false,true,false,false],[false,false,true,false],[false,false,false,true]])assert.equal(canDiscardDraft(nativeDraft,'binding',...flags),false);
console.log('discard admission: independent of body readiness, blocks unknown save, duplicate dialog and closure');

for(const changed of [ {...nativeDraft,phase:'saving'}, {...nativeDraft,phase:'uncertain'} ]) {
 assert.equal(canDiscardDraft(changed,'binding',false,false,false,false),false,'confirmation must recheck phase after dialog');
}
assert.equal(canDiscardDraft(nativeDraft,'binding',false,false,false,true),false,'confirmation must recheck import after dialog');
assert.equal(canDiscardDraft(nativeDraft,'binding',true,false,false,false),false,'confirmation must recheck begin after dialog');
assert.equal(canDiscardDraft(nativeDraft,'binding',false,true,false,false),false,'confirmation must recheck closing after dialog');
console.log('discard dialog race: changed save/import/begin/close state cannot retire the draft');
