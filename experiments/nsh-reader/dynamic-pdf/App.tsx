import { Clipboard, mount, signal, View } from "nsh";
import { AlertDialog } from "nsh/base";
import { DynamicReader, ReaderSurface, NativeEditor } from "qusor/pdf-surface";
import { WorkspaceShell } from "./WorkspaceShell";
import { ReaderViewport } from "./ReaderViewport";
import { CapturePanel } from "./CapturePanel";
import { ActionButton, Body, InlineStatus, color } from "./ui";
import { editNote, frameKey, pinned, selectedDraft, settle, submit, submitEditor, editorEventMatches, canDiscardDraft, canDiscardPreparation, activeInputEvent, deferInputFocus, chunkEnd, sourceUnavailable, type Draft } from "./reader-state";
import { editorInput, localInput, composition, deliver, deliveryReply, retryDelivery, inputCanSave, type EditorInput, type Delivery } from "./editor-input-state";
const parse=(raw:string)=>JSON.parse(raw);
function App(){
 const config=signal<any>(null),display=signal<any>(null),ready=signal("");
 const width=signal(1000),height=signal(1000),boxes=signal<number[][]>([]);
 const draft=signal<Draft|null>(null),lastSaved=signal<Draft|null>(null),auxiliary=signal(false);
 const importing=signal(false),importedOnly=signal(false),frameFailed=signal(false),resized=signal(false);
 const editor=signal<EditorInput|null>(null),beginning=signal(false),editorMessage=signal(""),bodyLoaded=signal(false),closingEditor=signal(false),nativeClosing=signal<{binding:string;pendingRelease:boolean}|null>(null),beginAttempt=signal(false);
 let activeApply:Promise<void>|null=null;
 const discardDialog=signal(false);
 const notice=signal(""),copyStatus=signal("");
 const status=signal("从本机打开 PDF，选文后可保存摘录或写想法。");
 let frameRequest=0,importRequest=0,focusEditor=false,focusEpoch=0;
 let pageRef:any=null,activeInputToken:object|null=null;
 const wide=()=>width()>=1024;
 const saving=()=>draft()?.phase==="saving";
 const locked=()=>pinned(draft())||!!editor()||beginning()||beginAttempt();
 const controlsEnabled=()=>!!config()&&!importing()&&!locked();
 const availableWidth=()=>Math.max(240,width()-(wide()&&auxiliary()?336:0)-(wide()?48:24));
 const resetSelection=()=>{draft.set(null);boxes.set([]);copyStatus.set("");};
 const clearImage=()=>{const old=display();display.set(null);if(old?.url)URL.revokeObjectURL(old.url);};
 const detach=()=>draft.set(sourceUnavailable(draft()));
 const clearNative=async()=>{
  const e=editor();if(!e){if(beginAttempt()){notice.set("编辑准备结果尚未确认，请重试准备后再关闭；当前内容可复制。");return false;}return !beginning();}
  closingEditor.set(true);
  try{if(activeApply){pauseInput();await activeApply;}
   if(editor()?.binding!==e.binding)throw Error();
   const r=parse(await NativeEditor.discard(e.binding));if(!r.ok||r.value?.binding!==e.binding)throw Error();
   nativeClosing.set({binding:e.binding,pendingRelease:r.value.state==="closing"&&r.value.pending_native_release_ack===true});
   if(nativeClosing()?.pendingRelease)notice.set("编辑器仍在关闭，可继续阅读。");
   editor.set(null);bodyLoaded.set(false);beginAttempt.set(false);editorMessage.set("编辑器仍在关闭，可继续阅读。");return true;
  }catch(_){notice.set("暂未能关闭这份想法。内容仍保留，可复制后稍后重试。");auxiliary.set(true);return false;}finally{closingEditor.set(false);}
 };
 const returnToText=async()=>{if(discardDialog())return;if(editor()?.composing){editorMessage.set("请先完成当前输入，再返回正文。内容仍保留在这里。");auxiliary.set(true);return;}activeInputToken=null;focusEditor=false;++focusEpoch;const old=draft();if(old?.phase==="saved"){if(!await clearNative())return;resetSelection();}else if(!editor()&&!beginning()&&old?.phase==="editing"&&!old.note.length&&!old.submission)draft.set({...old,phase:"selected"});auxiliary.set(false);setTimeout(()=>{pageRef?.focus();},0);};
 const discard=async()=>{
  const old=draft(),nativeBinding=editor()?.binding??"";
  if(!old||!canDiscardDraft(old,nativeBinding,beginning(),closingEditor(),discardDialog(),importing()))return;
  discardDialog.set(true);
  try{const answer=await AlertDialog.show({title:"放弃这份未保存想法？",description:"引文与想法只保留在当前会话。放弃后不会保存这份草稿。",actions:[{id:"keep",label:"继续编辑",role:"cancel"},{id:"discard",label:"放弃草稿",role:"destructive"}]});
  if(answer.action==="discard"&&draft()?.id===old.id&&draft()?.binding===old.binding&&editor()?.binding===nativeBinding&&canDiscardDraft(draft(),nativeBinding,beginning(),closingEditor(),false,importing())&&await clearNative()){resetSelection();discardDialog.set(false);returnToText();}
  }finally{discardDialog.set(false);}
 };
 const copyDraft=async()=>{const old=draft();if(!old)return;try{await Clipboard.writeText(`${old.title} · 第 ${old.page+1} 页\n\n${old.quote}\n\n我的想法\n${old.note}`);if(draft()?.id===old.id)copyStatus.set("已复制引文与想法。");}catch(_){if(draft()?.id===old.id)copyStatus.set("复制未完成，可选中想法内容手动复制。");}};
 const leaveUncertain=async()=>{const old=draft();if(old?.phase!=="uncertain")return;const answer=await AlertDialog.show({title:"离开这份结果未确认的草稿？",description:"保存可能已经完成。离开不会撤销已经发生的保存。建议先复制引文与想法；也可先原样重试确认结果。",actions:[{id:"keep",label:"返回草稿",role:"cancel"},{id:"leave",label:"离开此草稿"}]});if(answer.action==="leave"&&draft()?.id===old.id&&await clearNative()){resetSelection();returnToText();notice.set("已离开草稿，未撤销任何已发生的保存。");}};
 const requestImage=async(c:any)=>{
  const id=++frameRequest;config.set(c);ready.set("");frameFailed.set(false);clearImage();boxes.set([]);status.set("正在加载这一页…");
  try{const file=await DynamicReader.frame(c.handle,c.generation,c.frame_id);if(id!==frameRequest||frameKey(config())!==frameKey(c))return;
   const info=parse(file.info);if(frameKey(info)!==frameKey(c))throw Error("stale frame");display.set({frame:info,url:URL.createObjectURL(file.file)});
  }catch(_){if(id===frameRequest&&frameKey(config())===frameKey(c)){frameFailed.set(true);status.set("这一页暂未加载完成。原资料仍在本机，可重试当前页。");}}
 };
 const configure=(page:number,zoom:number)=>{
  if(!controlsEnabled())return;const old=config();const result=parse(ReaderSurface.configure(old.handle,old.generation,page,zoom));
  if(frameKey(config())!==frameKey(old))return;
  if(result.ok){resetSelection();auxiliary.set(false);resized.set(false);requestImage(result.value);}
  else{notice.set("阅读组件暂未就绪。资料与想法仍保留，可稍后重试。");}
 };
 const fit=(mode:"width"|"page",c=config())=>{
  if(!c)return .6;const baseWidth=c.width/c.zoom,baseHeight=c.height/c.zoom;
  return Math.max(.25,Math.min(1.25,availableWidth()/baseWidth,mode==="page"?Math.max(240,height()-180)/baseHeight:1.25));
 };
 const displayDocument=async(c:any,keepDetached:boolean)=>{
  if(!keepDetached){resetSelection();lastSaved.set(null);auxiliary.set(false);}else if(draft()){detach();auxiliary.set(true);}
  importedOnly.set(false);resized.set(false);
  const fitted=parse(ReaderSurface.configure(c.handle,c.generation,c.page,fit("page",c)));
  await requestImage(fitted.ok?fitted.value:c);
 };
 const open=async()=>{
  if(importing()||saving()||discardDialog())return;
  if(beginning()||(beginAttempt()&&!editor())){auxiliary.set(true);return;}
  const old=draft();if(old?.phase==="uncertain"){auxiliary.set(true);return;}
  if(old&&pinned(old)){
   const answer=await AlertDialog.show({title:"打开另一份资料？",description:"当前想法尚未保存。只有另一份资料成功打开后才会清除；取消或打开失败会保留。",actions:[{id:"keep",label:"继续当前阅读",role:"cancel"},{id:"open",label:"选择另一份…"}]});
   if(answer.action!=="open")return;const current=draft();if(current?.id!==old.id||current.binding!==old.binding||current.note!==old.note||current.phase!==old.phase){notice.set("想法已变化，当前内容已保留。请重新选择打开资料。");return;}
  }
  const id=++importRequest;notice.set("");importing.set(true);
  if(activeApply){pauseInput();await activeApply;if(id!==importRequest)return;}status.set("请选择本地 PDF，原文件保留在本机。");
  try{const result=parse(await DynamicReader.choose());if(id!==importRequest)return;
   if(!result.ok){notice.set("这份 PDF 未能打开。原资料与想法已保留，可检查文件或选择其他 PDF。");return;}
   if(result.value.cancelled){status.set(config()?"已取消，继续当前阅读。":"从本机打开 PDF 开始阅读。");return;}
   if(result.value.imported&&!result.value.opened){detach();if(draft())auxiliary.set(true);config.set(null);clearImage();boxes.set([]);importedOnly.set(true);notice.set("资料已保存到本机，阅读组件尚未打开。原想法仍按原来源保留。");return;}
   const cleaned=await clearNative();
   await displayDocument(result.value,!cleaned);
   if(!cleaned)notice.set("新资料已打开，旧想法尚未关闭。可复制旧内容或重试放弃；暂不能开始新想法。");
  }catch(_){if(id===importRequest)notice.set("未能打开资料。原资料与想法已保留。");}
  finally{if(id===importRequest){focusEditor=!!editor();++focusEpoch;importing.set(false);pump();}}
 };
 const reopen=async()=>{
  if(importing()||saving()||editor()||beginning())return;importing.set(true);
  try{const result=parse(await DynamicReader.reopen());if(result.ok)await displayDocument(result.value,!!draft());else notice.set("资料仍在本机，阅读组件暂不可用。原想法未清除。");}
  catch(_){notice.set("阅读组件暂不可用，可稍后重试。原想法未清除。");}finally{importing.set(false);}
 };
 const cancelImport=()=>{const result=parse(DynamicReader.cancel());if(result.ok&&result.value.cancelled){++importRequest;importing.set(false);status.set("已取消导入，当前阅读与想法保留。");}else status.set("正在完成本机保存，请稍候。");};
 const send=async(command:Delivery)=>{
  try{const result=parse(await NativeEditor.apply(command.binding,command.baseRevision,command.text));
   const e=editor();if(e)editor.set(deliveryReply(e,command,result));
  }catch(_){const e=editor();if(e)editor.set(deliveryReply(e,command,null));}
  const e=editor();if(!e)return;
  editorMessage.set(e.uncertain?"输入处理尚未确认。可继续写或复制；原样重试会先确认已交付内容。":e.rejectedText!==undefined?"这次输入未被接受。请缩短内容后再试，原文仍保留。":"");
  activeApply=null;pump();
 };
 const pump=()=>{const e=editor();if(!e||importing()||closingEditor()||!bodyLoaded())return;const next=deliver(e);if(next){editor.set(next.state);editorMessage.set("正在处理输入，最新文字保留在这里…");activeApply=send(next.command);}};
 const retryInput=()=>{const e=editor();if(!e||importing()||closingEditor()||beginning())return;const next=retryDelivery(e);if(next){editor.set(next.state);activeApply=send(next.command);}};
 const pauseInput=()=>{const e=editor();if(!e)return;try{const r=parse(NativeEditor.pause(e.binding));editorMessage.set(r.ok?"已请求暂停；正在等待处理结束，原输入保留，可原样重试。":"暂停尚未确认，原输入仍保留。");}catch(_){editorMessage.set("暂停尚未确认，原输入仍保留。");}};
 const write=async()=>{
  const old=draft();if(!old||discardDialog()||closingEditor())return;focusEditor=true;++focusEpoch;auxiliary.set(true);
  if((editor()&&bodyLoaded())||beginning()||old.phase==="saved"||old.submission)return;
  beginning.set(true);beginAttempt.set(true);editorMessage.set("正在准备想法，原选段保持不变…");
  try{const r=parse(await NativeEditor.begin(old.id,"a"));const v=r.value;
   if(!r.ok||typeof v?.binding!=="string"||!Number.isSafeInteger(v.revision))throw Error();
   // Retain binding before any further await: partial loading must never orphan a native slot.
   editor.set(editorInput(v.binding,v.revision,old.note));
   let body="";
   if(v.body_loaded===true&&typeof v.body==="string")body=v.body;
   else {
    let offset=0;const total=v.total_bytes;
    if(!Number.isSafeInteger(total)||total<0||total>1048576)throw Error();
    while(offset<total){const chunk=parse(await NativeEditor.read(v.binding,v.revision,offset,512));const c=chunk.value;
     if(!chunk.ok||c.binding!==v.binding||c.revision!==v.revision||c.offset!==offset||c.total_bytes!==total||typeof c.text!=="string")throw Error();
     offset=chunkEnd(offset,c.text,total);body+=c.text;
    }
   }
   if(draft()?.id!==old.id||draft()?.binding!==old.binding)throw Error();
   const local=draft()!.note;
   let loaded=editorInput(v.binding,v.revision,body);if(local!==old.note||local.length)loaded=localInput(loaded,v.binding,local);
   editor.set(loaded);bodyLoaded.set(true);beginAttempt.set(false);draft.set({...old,note:loaded.localText,phase:v.receipt?"saved":"editing",receipt:v.receipt});editorMessage.set("");pump();
  }catch(_){editorMessage.set("想法尚未准备完成。原内容已保留，请重试或复制；不会将空白当作正文。");}
  finally{beginning.set(false);}
 };
 const save=async()=>{
  const old=draft();if(!old||discardDialog()||closingEditor())return;if(importing()||(!old.submission?.editor&&(old.sourceUnavailable||old.binding!==frameKey(config())||ready()!==old.binding))){notice.set("原页面暂不可用，不能提交或重试。内容仍保留，可复制后明确离开草稿。");return;}
  const e=editor();if(beginAttempt()||beginning()||(e&&(!bodyLoaded()||(!old.submission&&!inputCanSave(e))))){notice.set("请先确认输入处理完成，再保存。内容仍保留。");return;}
  if(old.phase!=="selected"&&!e&&!old.submission){notice.set("编辑尚未就绪，内容已保留，请重新准备想法。");return;}
  const pending=e?submitEditor(old,e.binding,e.revision):submit(old);if(!pending)return;notice.set("");draft.set(pending);const frozen=pending.submission!;
  try{const result=parse(await (frozen.editor?NativeEditor.save(frozen.editor.binding,frozen.editor.revision):DynamicReader.save(frozen.candidate,frozen.note)));const receipt=result.ok&&(frozen.editor?result.value?.binding===frozen.editor.binding:result.value?.saved)?result.value.receipt:undefined;
   const next=settle(draft(),old.id,frozen.binding,receipt);draft.set(next);if(next?.phase==="saved")lastSaved.set(next);
  }catch(_){draft.set(settle(draft(),old.id,frozen.binding));}
 };
 const selection=(frame:any,raw:string)=>{
  if(frameKey(config())!==frameKey(frame)||importing()||editor()||beginning())return;const result=parse(raw);
  if(!result.ok){if(!locked())resetSelection();return;}const value=result.value;boxes.set(value.boxes??[]);
  if(value.candidate){draft.set(selectedDraft(frame,value.candidate.candidate_id,value.text??""));status.set("");}
 };
 const anchor=(frame:any)=>{
  const current=boxes();const left=current.length?Math.min(...current.map(b=>b[0])):16;
  const top=current.length?Math.min(...current.map(b=>b[1])):16;const bottom=current.length?Math.max(...current.map(b=>b[1]+b[3])):16;
  return {left:Math.max(8,Math.min(left,frame.width-340)),top:bottom+112<frame.height?bottom+12:Math.max(8,top-108)};
 };
 const context=(frame:any)=><If when={!!draft()&&draft()?.binding===frameKey(frame)&&!auxiliary()}><View style={{position:"absolute",...anchor(frame),width:Math.min(332,frame.width-16),padding:8,gap:8,borderWidth:1,borderRadius:10,borderColor:color("border-subtle"),backgroundColor:color("surface-content")}}>
  <If when={draft()?.phase==="selected"}><View style={{flexDirection:"row",gap:8}}><ActionButton label="保存摘录" primary onPress={save}/><ActionButton label="写想法" onPress={write}/></View></If>
  <If when={draft()?.phase==="editing"}><InlineStatus>有一份未保存的想法</InlineStatus><View style={{flexDirection:"row",gap:8}}><ActionButton label="继续写想法" onPress={write}/><ActionButton label="放弃草稿…" onPress={discard}/></View></If>
  <If when={draft()?.phase==="saving"}><InlineStatus>正在保存，内容已保留…</InlineStatus></If>
  <If when={draft()?.phase==="uncertain"}><InlineStatus danger>保存尚未确认，需原样重试。</InlineStatus><ActionButton label="查看保存状态" onPress={write}/></If>
  <If when={draft()?.phase==="saved"}><InlineStatus success>已保存到本机</InlineStatus><View style={{flexDirection:"row",gap:8}}><ActionButton label="继续阅读" primary onPress={returnToText}/><ActionButton label="查看这次保存" onPress={write}/></View></If>
 </View></If>;
 const toolbar=<View style={{gap:8}}>
  <View style={{flexDirection:"row",alignItems:"center",gap:12}}><View style={{flex:1,minWidth:0}}><Body title>{config()?.title||"本地阅读"}</Body></View><ActionButton label={config()?"打开另一份…":"打开 PDF…"} compact disabled={importing()||saving()} onPress={open}/><If when={!!draft()&&draft()?.phase!=="selected"}><ActionButton label={draft()?.phase==="detached"?"原资料草稿":draft()?.phase==="saved"?"这次保存":"想法草稿"} compact onPress={write}/></If><If when={!draft()&&!!lastSaved()}><ActionButton label="这次保存" compact onPress={()=>{draft.set(lastSaved());auxiliary.set(true);}}/></If></View>
  <If when={!!config()}><View style={{flexDirection:"row",flexWrap:"wrap",alignItems:"center",gap:8}}>
   <ActionButton label="上一页" compact onPress={()=>configure(config().page-1,config().zoom)} disabled={!controlsEnabled()||config().page===0}/><Body muted>{`${config().page+1} / ${config().pages}`}</Body><ActionButton label="下一页" compact onPress={()=>configure(config().page+1,config().zoom)} disabled={!controlsEnabled()||config().page+1>=config().pages}/>
   <View style={{width:8}}/><ActionButton label="缩小" compact disabled={!controlsEnabled()} onPress={()=>configure(config().page,Math.max(.25,config().zoom-.1))}/><Body muted>{`${Math.round(config().zoom*100)}%`}</Body><ActionButton label="放大" compact disabled={!controlsEnabled()} onPress={()=>configure(config().page,Math.min(1.25,config().zoom+.1))}/><ActionButton label="适合宽度" compact disabled={!controlsEnabled()} onPress={()=>configure(config().page,fit("width"))}/><ActionButton label="整页" compact disabled={!controlsEnabled()} onPress={()=>configure(config().page,fit("page"))}/>
  </View></If>
 </View>;
 const content=<View style={{flex:1,gap:12}}>
  <If when={importing()}><View style={{flexDirection:"row",alignItems:"center",gap:12}}><View style={{flex:1}}><InlineStatus>正在打开资料，当前内容保留…</InlineStatus></View><ActionButton label="取消导入" compact onPress={cancelImport}/></View></If>
  <If when={!display()}><View style={{flex:1,justifyContent:"center",alignItems:"flex-start",padding:24,gap:20}}><Body title>{importedOnly()?"资料已导入，阅读尚未就绪":config()?"页面暂未就绪":"让阅读留在正文里"}</Body><Body>{status()}</Body><Body muted>支持 16 MiB、16 页以内的 PDF。原文件不上传。</Body><If when={importedOnly()}><ActionButton label="重新打开已导入资料" primary disabled={importing()} onPress={reopen}/></If><If when={!importedOnly()}><ActionButton label={config()?"重试当前页":"打开本地 PDF…"} primary disabled={importing()||locked()} onPress={()=>config()?configure(config().page,config().zoom):open()}/></If></View></If>
  <Each of={display()?[display()]:[]} key={(entry:any)=>`${frameKey(entry.frame)}:${entry.url}`}>{(entry)=>{const item=entry(),frame=item.frame;const matches=()=>frameKey(config())===frameKey(frame)&&display()?.url===item.url;return <ReaderViewport entry={item} covered={!wide()&&auxiliary()} interactive={()=>matches()&&!importing()&&!saving()&&(wide()||!auxiliary())} canSelect={()=>!locked()} ready={()=>ready()===frameKey(frame)} boxes={boxes} context={context(frame)}
   pageRef={(node)=>pageRef=node}
   onSelectionStart={()=>{if(editor()||beginning())return;notice.set("");resetSelection();auxiliary.set(false);}} onGestureCancel={()=>{if(!locked())resetSelection();}}
   receive={(raw)=>selection(frame,raw)}
   loaded={()=>{if(!matches()||ready()===frameKey(frame))return;const result=parse(ReaderSurface.ready(frame.handle,frame.generation,frame.frame_id,frame.report_digest));if(result.ok&&matches()){ready.set(frameKey(frame));status.set(frame.text_status==="mapped"?"拖动选择正文，保存摘录或写下自己的想法。":frame.text_status==="no_text"?"此页没有可选文字。":"此页暂不支持精确选文。");}}}
   failed={()=>{if(!matches())return;ReaderSurface.failed(frame.handle,frame.generation,frame.frame_id);ready.set("");boxes.set([]);detach();frameFailed.set(true);status.set("页面加载失败，原资料与想法仍保留。");}}/>;}}</Each>
  <If when={frameFailed()&&!!display()}><InlineStatus danger>{status()}</InlineStatus><ActionButton label="重试当前页" disabled={!controlsEnabled()} onPress={()=>configure(config().page,config().zoom)}/></If>
 </View>;
 return <WorkspaceShell toolbar={toolbar} content={content} auxiliaryOpen={auxiliary()} wide={wide()}
  auxiliary={<CapturePanel draft={draft} close={returnToText} discard={discard} save={save} copy={copyDraft} copyStatus={copyStatus()} leaveUncertain={leaveUncertain} editingEnabled={!importing()&&!closingEditor()&&!beginning()&&bodyLoaded()&&!!editor()} saveEnabled={!discardDialog()&&!closingEditor()&&!importing()&&!beginning()&&bodyLoaded()&&!!editor()&&inputCanSave(editor()!)} deliveryStatus={editorMessage()} canPause={!!editor()?.inflight&&!editor()?.uncertain&&!closingEditor()} canRetry={!!editor()?.uncertain&&!!editor()?.inflight&&!editor()?.composing&&!importing()&&!closingEditor()} preparing={beginning()} canDiscard={canDiscardDraft(draft(),editor()?.binding??"",beginning(),closingEditor(),discardDialog(),importing())} canDiscardPreparation={canDiscardPreparation(editor()?.binding??"",bodyLoaded(),beginning())&&canDiscardDraft(draft(),editor()?.binding??"",beginning(),closingEditor(),discardDialog(),importing())} needsPreparation={!bodyLoaded()} retryInput={()=>{if(!bodyLoaded())void write();else retryInput();}} pauseInput={pauseInput} nativeBinding={editor()?.binding??""} composition={(id,binding,nativeBinding,token,value)=>{const e=editor();if(!importing()&&!closingEditor()&&bodyLoaded()&&activeInputEvent(activeInputToken,token,auxiliary())&&editorEventMatches(draft(),e?.binding,id,binding,nativeBinding)&&e){editor.set(composition(e,nativeBinding,value));if(!value)pump();}}} retryEnabled={!importing()&&(!!draft()?.submission?.editor||(!draft()?.sourceUnavailable&&draft()?.binding===frameKey(config())&&ready()===draft()?.binding))} inputRef={(node,editable,token)=>{
   if(!node)return;activeInputToken=token;if(!editable)return;
   const epoch=focusEpoch,e=editor(),d=draft();if(!e||!d)return;
   const nativeBinding=e.binding,id=d.id,binding=d.binding;
   console.log("EDITOR_FOCUS_SCHEDULED");
   deferInputFocus((work,delay)=>setTimeout(work,delay),()=>focusEditor&&focusEpoch===epoch&&!importing()&&!beginning()&&!closingEditor()&&bodyLoaded()&&activeInputEvent(activeInputToken,token,auxiliary())&&editorEventMatches(draft(),editor()?.binding,id,binding,nativeBinding),()=>{node.focus();console.log("EDITOR_FOCUS_REQUESTED");focusEditor=false;});
  }} change={(id,binding,nativeBinding,token,value)=>{const e=editor();if(!importing()&&!beginning()&&!closingEditor()&&bodyLoaded()&&activeInputEvent(activeInputToken,token,auxiliary())&&editorEventMatches(draft(),e?.binding,id,binding,nativeBinding)&&e){draft.set(editNote(draft(),id,binding,value));editor.set(localInput(e,nativeBinding,value));pump();}}}/>}
  status={<InlineStatus>{notice()||(!draft()?(resized()&&config()?"窗口已调整，当前页保持原倍率；可按需选择适合宽度。":display()?status():""):"")}</InlineStatus>}
  onLayout={({bounds}:any)=>{if(bounds.size.width<=0||bounds.size.height<=0)return;if(width()!==bounds.size.width||height()!==bounds.size.height){width.set(bounds.size.width);height.set(bounds.size.height);if(config())resized.set(true);}}}/>;
}
mount(App);
