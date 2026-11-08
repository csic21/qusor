import { Canvas, Image, Scroll, View } from "nsh";
import { ReaderSurface } from "qusor/pdf-surface";
import { color } from "./ui";
/** Native pixels and pointer hit-testing remain within the trusted host UI. */
export function ReaderViewport(props:{entry:any;covered:boolean;interactive:()=>boolean;canSelect:()=>boolean;ready:()=>boolean;boxes:()=>number[][];onSelectionStart:()=>void;onGestureCancel:()=>void;receive:(raw:string)=>void;loaded:()=>void;failed:()=>void;context:any;pageRef:(node:any)=>void}) {
 const frame=props.entry.frame;let gesture:number|null=null,pointer:number|null=null;
 const receive=(raw:string)=>{if(!props.interactive())return;const result=JSON.parse(raw);if(result.ok){const value=result.value;if(value.handle!==frame.handle||value.generation!==frame.generation||value.frame_id!==frame.frame_id)return;gesture=value.gesture??null;}else{gesture=null;pointer=null;}props.receive(raw);};
 const cancel=()=>{const previousGesture=gesture,previousPointer=pointer;gesture=null;pointer=null;if(previousGesture!==null&&previousPointer!==null){try{ReaderSurface.cancel(frame.handle,frame.generation,previousPointer,previousGesture);}finally{props.onGestureCancel();}}};
 return <Scroll focusable={false} style={{flex:1,pointerEvents:props.covered?"none":"auto"}} onScroll={cancel} contentStyle={{minWidth:"100%"}}>
  <Scroll focusable={false} horizontal style={{width:"100%",height:frame.height}} contentStyle={{minWidth:"100%",alignItems:"center",justifyContent:"center"}} onScroll={cancel}>
   <View ref={props.pageRef} focusable={!props.covered} accessibilityLabel={`正文，第 ${frame.page+1} 页`} style={{width:frame.width,height:frame.height,position:"relative"}}>
    <Image src={props.entry.url} onLoad={props.loaded} onError={props.failed} style={{width:frame.width,height:frame.height}}/>
    <Canvas onStartShouldSetResponder={()=>props.interactive()&&props.ready()&&props.canSelect()&&frame.text_status==="mapped"}
     onResponderGrant={(e:any)=>{if(!props.interactive()||!props.canSelect())return;props.onSelectionStart();pointer=e.identifier;receive(ReaderSurface.down(frame.handle,frame.generation,pointer,e.locationX,e.locationY));}}
     onResponderMove={(e:any)=>{if(gesture!==null&&pointer===e.identifier)receive(ReaderSurface.motion(frame.handle,frame.generation,pointer,gesture,e.locationX,e.locationY));}}
     onResponderRelease={(e:any)=>{try{if(gesture!==null&&pointer===e.identifier)receive(ReaderSurface.up(frame.handle,frame.generation,pointer,gesture,e.locationX,e.locationY));}finally{gesture=null;pointer=null;}}}
     onResponderTerminate={cancel} onResponderTerminationRequest={()=>true} onMouseLeave={cancel}
     draw={(cx)=>{cx.fillStyle=color("highlight-overlay");for(const [x,y,w,h]of props.boxes())cx.fillRect(x,y,w,h);}}
     style={{position:"absolute",left:0,top:0,width:frame.width,height:frame.height}}/>
    {props.context}
   </View>
  </Scroll>
 </Scroll>;
}
