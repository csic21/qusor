import { Canvas, Image, Scroll, mount, signal, Text, View } from "nsh";
import { Input } from "nsh/base";
import { ReaderSurface } from "qusor/pdf-surface";
import { reportDigest } from "./frame";
import page0 from "./assets/page-0.png";
import page1 from "./assets/page-1.png";
const parse=(text:string)=>JSON.parse(text);
const initial=parse(ReaderSurface.open(reportDigest));
if(!initial.ok)throw Error(initial.error);
function App(){
 const config=signal(initial.value);const width=signal(800);const ready=signal("");
 const boxes=signal<number[][]>([]);const quote=signal("");const candidate=signal("");const saved=signal(false);const note=signal("");let noteCommitted=false;
 const status=signal("页面加载中");const disabled=signal(false);let layout="";let desiredZoom=.6;
 const key=(c:any)=>`${c.handle}:${c.generation}:${c.frame_id}`;
 const wide=()=>width()>=900;const paperWidth=()=>wide()?width()-276:width();
 const clear=()=>{boxes.set([]);quote.set("");candidate.set("");saved.set(false);};
 const configure=(page:number,zoom:number,fromLayout=false)=>{
  if(!fromLayout)desiredZoom=zoom;const old=config();const result=parse(ReaderSurface.configure(old.handle,old.generation,page,zoom));
  if(key(config())!==key(old))return;
  clear();ready.set("");if(result.ok){config.set(result.value);status.set("页面加载中");}else status.set("页面暂不可用，请重新打开。");
 };
 const button=(title:string,action:()=>void,enabled=true)=><View accessibilityState={{disabled:!enabled}} onPress={enabled?action:undefined} style={{paddingHorizontal:12,paddingVertical:9,borderRadius:6,backgroundColor:"#e8e7e3",opacity:enabled?1:.45}}><Text style={{fontSize:14}}>{title}</Text></View>;
 return <View onLayout={({bounds}:any)=>{if(bounds.size.width<=0||bounds.size.height<=0)return;const size=`${bounds.size.width}:${bounds.size.height}`;width.set(Math.max(240,bounds.size.width-32));if(!layout||layout!==size)configure(config().page,Math.max(.25,Math.min(desiredZoom,paperWidth()/(config().width/config().zoom))),true);layout=size;}} style={{flex:1,padding:16,gap:12,backgroundColor:"#f2f1ed"}}>
  <View style={{flexDirection:"row",flexWrap:"wrap",alignItems:"center",gap:8}}>
   <Text style={{flex:1,fontSize:18,fontWeight:"600"}}>text-two-pages.pdf</Text>
   {button("上一页",()=>configure(config().page-1,config().zoom),!disabled()&&config().page>0)}
   <Text>{`${config().page+1} / ${config().pages}`}</Text>
   {button("下一页",()=>configure(config().page+1,config().zoom),!disabled()&&config().page+1<config().pages)}
   <Text>{`${Math.round(config().zoom*100)}%`}</Text>
   {button("−",()=>configure(config().page,Math.max(.25,config().zoom-.1)),!disabled())}
   {button("＋",()=>configure(config().page,Math.min(1.25,config().zoom+.1)),!disabled())}
  </View>
  <Each of={[config()]} key={key}>{(entry)=>{const frame=entry();let gesture:number|null=null;let pointer:number|null=null;
   const current=()=>key(config())===key(frame)&&!disabled();
   const receive=(raw:string)=>{if(!current())return;const r=parse(raw);if(!r.ok){clear();gesture=null;pointer=null;status.set("拖动正文以选择文字");return;}const v=r.value;if(v.handle!==frame.handle||v.generation!==frame.generation||v.frame_id!==frame.frame_id)return;gesture=v.gesture??null;boxes.set(v.boxes??[]);quote.set(v.text??"");candidate.set(v.candidate?.candidate_id??"");saved.set(false);status.set(v.candidate?"候选引文 · 尚未保存":v.text?"正在选择文字":"拖动正文以选择文字");};
   const cancel=()=>{if(gesture!==null){ReaderSurface.cancel(frame.handle,frame.generation,pointer!,gesture);gesture=null;pointer=null;if(current()){clear();status.set("拖动正文以选择文字");}}};
   return <Scroll style={{flex:1}} contentStyle={{flexDirection:wide()?"row":"column",gap:16,alignItems:"flex-start"}} onScroll={cancel}>
    <Scroll horizontal style={{width:paperWidth(),height:frame.height}} onScroll={cancel}>
     <View style={{width:frame.width,height:frame.height,backgroundColor:"#ffffff"}}>
      <Image src={frame.page===0?page0:page1} onLoad={()=>{if(!current())return;const r=parse(ReaderSurface.ready(frame.handle,frame.generation,frame.frame_id,reportDigest));if(r.ok&&current()){ready.set(key(frame));status.set("拖动正文以选择文字");}}} onError={()=>{if(!current())return;ReaderSurface.failed(frame.handle,frame.generation,frame.frame_id);ready.set("");clear();status.set("页面加载失败，请重新打开。");}} style={{width:frame.width,height:frame.height}}/>
      <Canvas onStartShouldSetResponder={()=>current()&&ready()===key(frame)} onResponderGrant={(e:any)=>{if(!current())return;if(noteCommitted){note.set("");noteCommitted=false;}clear();pointer=e.identifier;receive(ReaderSurface.down(frame.handle,frame.generation,pointer,e.locationX,e.locationY));}} onResponderMove={(e:any)=>{if(gesture!==null&&pointer===e.identifier)receive(ReaderSurface.motion(frame.handle,frame.generation,pointer,gesture,e.locationX,e.locationY));}} onResponderRelease={(e:any)=>{if(gesture!==null&&pointer===e.identifier)receive(ReaderSurface.up(frame.handle,frame.generation,pointer,gesture,e.locationX,e.locationY));pointer=null;}} onResponderTerminate={cancel} onResponderTerminationRequest={()=>true} onMouseLeave={cancel} draw={(cx)=>{cx.fillStyle="rgba(245,196,70,0.35)";for(const [x,y,w,h]of boxes())cx.fillRect(x,y,w,h);}} style={{position:"absolute",left:0,top:0,width:frame.width,height:frame.height}}/>
     </View>
    </Scroll>
    <View style={{width:wide()?260:width(),padding:16,gap:12,borderRadius:8,backgroundColor:"#ffffff"}}>
     <Text style={{fontSize:13,color:"#666666"}}>{`text-two-pages.pdf · 第 ${frame.page+1} 页`}</Text>
     <Text style={{fontSize:14,color:"#666666"}}>{status()}</Text>
     <Text style={{fontSize:16,lineHeight:24}}>{quote()||"在页面中拖动选择文字，确认引文后保存。"}</Text>
     <Input value={note()} onChange={({value}:any)=>{note.set(value);noteCommitted=false;}} multiline readonly={saved()} placeholder="个人想法" style={{height:96,padding:8,borderWidth:1,borderColor:"#dddddd"}}/>
     {button(saved()?"已保存":"保存摘录",()=>{const id=candidate();const r=parse(ReaderSurface.save(id,note()));if(!current()||candidate()!==id)return;if(r.ok&&r.value.saved){saved.set(true);noteCommitted=true;status.set("已保存到本机");}else status.set("保存未完成，请重试或重新选择。");},!!candidate()&&!saved()&&!disabled())}
     <Text style={{fontSize:12,color:"#777777"}}>普通签名包与公开 Surface 能力资格验证。</Text>
    </View>
   </Scroll>;}}
  </Each>
 </View>;
}
mount(App);
