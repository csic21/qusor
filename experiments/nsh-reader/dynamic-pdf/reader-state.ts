/** UI-only session state; never serializes a Host lease or claims a durable draft. */
export type DraftPhase="selected"|"editing"|"saving"|"uncertain"|"saved"|"detached";
export type Draft={id:string;binding:string;title:string;page:number;quote:string;note:string;phase:DraftPhase;submission?:{candidate:string;note:string;binding:string;editor?:{binding:string;revision:number}};receipt?:unknown;sourceUnavailable?:boolean};
export const frameKey=(c:any):string=>c?`${c.document_id}:${c.report_digest}:${c.handle}:${c.generation}:${c.frame_id}`:"empty";
export function selectedDraft(frame:any,candidate:string,quote:string):Draft{return {id:candidate,binding:frameKey(frame),title:frame.title,page:frame.page,quote,note:"",phase:"selected"};}
export const pinned=(draft:Draft|null):boolean=>!!draft&&["editing","saving","uncertain","detached"].includes(draft.phase);
export function editNote(draft:Draft|null,id:string,binding:string,note:string):Draft|null{return draft?.id===id&&draft.binding===binding&&draft.phase==="editing"?{...draft,note}:draft;}
export function submit(draft:Draft):Draft|null {
 if(draft.sourceUnavailable||!["selected","editing","uncertain"].includes(draft.phase))return null;
 return {...draft,phase:"saving",submission:draft.submission??{candidate:draft.id,note:draft.note,binding:draft.binding}};
}
export function settle(draft:Draft|null,id:string,binding:string,receipt?:unknown):Draft|null {
 if(draft?.id!==id||draft.binding!==binding||draft.phase!=="saving")return draft;
 return {...draft,phase:receipt?"saved":"uncertain",receipt};
}

export function sourceUnavailable(draft:Draft|null):Draft|null {
 if(!draft)return null;
 if(["saving","uncertain","saved"].includes(draft.phase))return {...draft,sourceUnavailable:true};
 return {...draft,phase:"detached",sourceUnavailable:true,submission:undefined};
}

/** Freeze a native save exactly once, including its immutable application revision. */
export function submitEditor(draft:Draft,binding:string,revision:number):Draft|null {
 if(draft.phase==="uncertain"&&draft.submission?.editor)return {...draft,phase:"saving"};
 const next=submit(draft);if(!next)return null;
 if(next.submission!.editor)return next;
 return {...next,submission:{...next.submission!,editor:{binding,revision}}};
}

export function editorEventMatches(draft:Draft|null,nativeBinding:string|undefined,id:string,frameBinding:string,eventNativeBinding:string):boolean {
 return !!draft&&draft.id===id&&draft.binding===frameBinding&&draft.phase==="editing"&&!!nativeBinding&&nativeBinding===eventNativeBinding;
}
/** Pinned DraftTexts reads are at most 512 UTF-8 bytes; never count JS UTF-16 units. */
export function chunkEnd(offset:number,text:string,total:number):number {
 const bytes=new TextEncoder().encode(text).length;
 if(!bytes||bytes>512||offset+bytes>total)throw Error("invalid pinned body chunk");
 return offset+bytes;
}

/** UI-instance identity is opaque and compared by reference, never serialized. */
export function activeInputEvent(active:object|null,event:object,visible:boolean):boolean {
 return visible&&active!==null&&active===event;
}

/** Yield past ref registration and the native mount/prop batch before requesting focus.
 * This schedules a request, not proof of native focus or painting. */
export function deferInputFocus(schedule:(work:()=>void,delay:number)=>unknown,valid:()=>boolean,focus:()=>void):void {
 schedule(()=>{if(valid())schedule(()=>{if(valid())focus();},32);},0);
}

/** All Input lifecycle dependencies belong in Each.of, whose read is tracked. */
export type EditorRow={draft:Draft;nativeBinding:string;editable:boolean};
export function editorRows(draft:Draft|null,nativeBinding:string,enabled:boolean):EditorRow[] {
 return draft?[{draft,nativeBinding,editable:enabled&&draft.phase==="editing"}]:[];
}
export function editorRowKey(row:EditorRow):string {
 return `${row.draft.id}:${row.draft.binding}:${row.nativeBinding}:${row.editable}`;
}

/** An acquired binding can be explicitly discarded after preparation fails. */
export function canDiscardPreparation(nativeBinding:string,bodyLoaded:boolean,preparing:boolean):boolean {
 return !!nativeBinding&&!bodyLoaded&&!preparing;
}

/** Discard admission is independent of text-input readiness. */
export function canDiscardDraft(draft:Draft|null,nativeBinding:string,preparing:boolean,closing:boolean,dialogOpen:boolean,importing:boolean):boolean {
 return !!draft&&!!nativeBinding&&draft.phase!=="saving"&&draft.phase!=="uncertain"&&!preparing&&!closing&&!dialogOpen&&!importing;
}
