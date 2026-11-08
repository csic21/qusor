import { Canvas, Image, Scroll, mount, signal, Text, View } from "nsh";
import { PdfSurface } from "qusor/pdf-surface";
import { reportDigest } from "./frame";
import page0 from "./assets/page-0.png";
import page1 from "./assets/page-1.png";

const parse = (text: string) => JSON.parse(text);
const initial = parse(PdfSurface.configure(0, 0.6, reportDigest));
if (!initial.ok) throw Error(initial.error);
function App() {
  const config = signal(initial.value);
  const availableWidth=signal(800);
  const readyGeneration=signal(0);
  const boxes = signal<number[][]>([]);
  const quote = signal("");
  const status = signal("拖动正文以选择文字");
  let gesture: number | null = null;
  let pointer: number | null = null;
  let layoutSize = "";
  let desiredZoom=.6;
  const wide=()=>availableWidth()>=900;
  const paperWidth=()=>wide()?availableWidth()-276:availableWidth();
  const clear = () => { gesture=null;pointer=null; boxes.set([]); quote.set(""); status.set("拖动正文以选择文字"); };
  const configure = (page: number, zoom: number,fromLayout=false) => {
    if(!fromLayout)desiredZoom=zoom;
    const result=parse(PdfSurface.configure(page,zoom,reportDigest));
    clear();
    if(result.ok) {readyGeneration.set(0);config.set(result.value);} else status.set("页面暂不可用");
  };
  const receive = (raw: string) => {
    const result=parse(raw);
    if(!result.ok) { clear(); return; }
    if(result.value.generation!==config().generation) return;
    gesture=result.value.gesture ?? null;
    boxes.set(result.value.boxes ?? []);
    quote.set(result.value.text ?? "");
    status.set(result.value.text ? "候选引文 · 尚未保存" : "拖动正文以选择文字");
  };
  const down=(e:any)=>{ clear();pointer=e.identifier;receive(PdfSurface.down(config().generation,pointer,e.locationX,e.locationY)); };
  const move=(e:any)=>{ if(gesture!==null && pointer===e.identifier) receive(PdfSurface.motion(config().generation,pointer,gesture,e.locationX,e.locationY)); };
  const up=(e:any)=>{ if(gesture!==null && pointer===e.identifier) receive(PdfSurface.up(config().generation,pointer,gesture,e.locationX,e.locationY));pointer=null; };
  const cancel=()=>{if(gesture!==null)PdfSurface.cancel(config().generation);clear();};
  const button=(label:string,action:()=>void,enabled=true)=><View accessibilityState={{disabled:!enabled}} onPress={enabled?action:undefined} style={{opacity:enabled?1:.45,paddingHorizontal:12,paddingVertical:9,borderRadius:6,backgroundColor:"#e8e7e3"}}><Text style={{fontSize:14}}>{label}</Text></View>;
  return <View onLayout={({bounds}:any)=>{if(bounds.size.width<=0 || bounds.size.height<=0)return;const size=`${bounds.size.width}:${bounds.size.height}`;const width=Math.max(240,bounds.size.width-32);availableWidth.set(width);if(!layoutSize || size!==layoutSize)configure(config().page,Math.max(.25,Math.min(desiredZoom,paperWidth()/(config().width/config().zoom))),true);layoutSize=size;}} style={{flex:1,backgroundColor:"#f2f1ed",padding:16,gap:12}}>
    <View style={{flexDirection:"row",flexWrap:"wrap",alignItems:"center",gap:8}}>
      <Text style={{fontSize:18,fontWeight:"600",flex:1}}>text-two-pages.pdf</Text>
      {button("上一页",()=>configure(Math.max(0,config().page-1),config().zoom),config().page>0)}
      <Text>{`${config().page+1} / ${config().pages}`}</Text>
      {button("下一页",()=>configure(Math.min(config().pages-1,config().page+1),config().zoom),config().page+1<config().pages)}
      <Text>{`${Math.round(config().zoom*100)}%`}</Text>
      {button("−",()=>configure(config().page,Math.max(.25,config().zoom-.1)))}
      {button("＋",()=>configure(config().page,Math.min(1.25,config().zoom+.1)))}
    </View>
    <Scroll style={{flex:1}} contentStyle={{flexDirection:wide()?"row":"column",gap:16,alignItems:"flex-start"}} onScroll={()=>{if(gesture!==null){PdfSurface.cancel(config().generation);clear();}}}>
      <Scroll horizontal style={{width:paperWidth(),height:config().height}} onScroll={()=>{if(gesture!==null){PdfSurface.cancel(config().generation);clear();}}}>
      <View style={{width:config().width,height:config().height,backgroundColor:"#ffffff"}}>
        <Each of={[{page:config().page,generation:config().generation}]} key={(item)=>item.generation}>{(entry)=>{const binding=entry();return <Image src={binding.page===0?page0:page1} onLoad={()=>{if(config().generation===binding.generation)readyGeneration.set(binding.generation);}} onError={()=>{if(config().generation===binding.generation){readyGeneration.set(0);status.set("页面加载失败，请重新打开。");}}} style={{width:config().width,height:config().height}} />;}}</Each>
        <Canvas onStartShouldSetResponder={()=>readyGeneration()===config().generation} onResponderGrant={down} onResponderMove={move} onResponderRelease={up} onResponderTerminate={cancel} onResponderTerminationRequest={()=>true}
          onMouseLeave={()=>{ if(gesture!==null){PdfSurface.cancel(config().generation);clear();} }}
          draw={(cx)=>{cx.fillStyle="rgba(245,196,70,0.35)";for(const [x,y,w,h] of boxes())cx.fillRect(x,y,w,h);}}
          style={{position:"absolute",left:0,top:0,width:config().width,height:config().height}} />
      </View>
      </Scroll>
      <View style={{width:wide()?260:availableWidth(),padding:16,borderRadius:8,backgroundColor:"#ffffff",gap:12}}>
        <Text style={{fontSize:13,color:"#666666"}}>{`text-two-pages.pdf · 第 ${config().page+1} 页`}</Text>
        <Text style={{fontSize:14,color:"#666666"}}>{status()}</Text>
        <Text style={{fontSize:16,lineHeight:24}}>{quote() || "在页面中拖动选择；选文会在这里预览。"}</Text>
        <Text style={{fontSize:12,color:"#777777"}}>原生 Surface 技术验证；当前未连接持久保存。</Text>
      </View>
    </Scroll>
  </View>;
}
mount(App);
