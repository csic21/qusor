/** UI delivery state. It holds user input, never a Host grant or an application Draft. */
export type Delivery = { binding:string; sequence:number; baseRevision:number; text:string };
export type EditorInput = {
  binding:string; revision:number; localText:string; appliedText:string;
  composing:boolean; nextSequence:number; inflight?:Delivery; uncertain:boolean; rejectedText?:string;
};
export function editorInput(binding:string,revision:number,body:string):EditorInput {
  return {binding,revision,localText:body,appliedText:body,composing:false,nextSequence:1,uncertain:false};
}
export function localInput(state:EditorInput,binding:string,text:string):EditorInput {
  return state.binding===binding?{...state,localText:text,rejectedText:text===state.rejectedText?state.rejectedText:undefined}:state;
}
export function composition(state:EditorInput,binding:string,composing:boolean):EditorInput {
  return state.binding===binding?{...state,composing}:state;
}
export function deliver(state:EditorInput):{state:EditorInput;command:Delivery}|null {
  if(state.composing||state.inflight||state.uncertain||state.localText===state.appliedText||state.localText===state.rejectedText)return null;
  if(!Number.isSafeInteger(state.nextSequence+1))return null;
  const command={binding:state.binding,sequence:state.nextSequence,baseRevision:state.revision,text:state.localText};
  return {state:{...state,inflight:command,nextSequence:state.nextSequence+1},command};
}
function matches(state:EditorInput,command:Delivery):boolean {
  const active=state.inflight;
  return state.binding===command.binding&&!!active&&active.sequence===command.sequence&&
    active.baseRevision===command.baseRevision&&active.text===command.text;
}
/** A response updates the applied baseline; it never replaces newer local input. */
export function applied(state:EditorInput,command:Delivery,revision:number):EditorInput {
  if(!matches(state,command))return state;
  if(revision!==command.baseRevision+1)return {...state,uncertain:true};
  return {...state,revision,appliedText:command.text,inflight:undefined,uncertain:false,rejectedText:undefined};
}
/** Use only an explicit native rejection proven to precede publication/effects. */
export function rejected(state:EditorInput,command:Delivery):EditorInput {
  return matches(state,command)?{...state,inflight:undefined,uncertain:false,rejectedText:command.text}:state;
}
/** An error is not proof of no effect. Retain the exact delivered snapshot. */
export function unconfirmed(state:EditorInput,command:Delivery):EditorInput {
  return matches(state,command)?{...state,uncertain:true}:state;
}
/** Treat malformed, busy and transport-unknown replies as possibly effective. */
export function deliveryReply(state:EditorInput,command:Delivery,result:unknown):EditorInput {
  if(!matches(state,command))return state;
  if(!result||typeof result!=="object")return unconfirmed(state,command);
  const reply=result as {ok?:unknown;value?:{binding?:unknown;revision?:unknown;applied?:unknown};code?:unknown;effect_unknown?:unknown};
  if(reply.ok===true&&reply.value?.binding===command.binding&&reply.value.applied===true&&
    typeof reply.value.revision==="number"&&Number.isSafeInteger(reply.value.revision)) {
    return applied(state,command,reply.value.revision);
  }
  if(reply.ok===false&&reply.code==="input_rejected"&&reply.effect_unknown===false) {
    return rejected(state,command);
  }
  return unconfirmed(state,command);
}
/** Native must reconcile/resume this same payload before accepting another one. */
export function retryDelivery(state:EditorInput):{state:EditorInput;command:Delivery}|null {
  if(!state.uncertain||!state.inflight||state.composing)return null;
  return {state:{...state,uncertain:false},command:state.inflight};
}
export function inputCanSave(state:EditorInput):boolean {
  return !state.composing&&!state.inflight&&!state.uncertain&&state.localText===state.appliedText;
}
