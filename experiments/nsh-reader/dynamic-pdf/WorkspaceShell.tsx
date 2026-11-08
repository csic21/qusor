import { View } from "nsh";
import { color } from "./ui";
/** Trusted native regions. This is props composition, not a public provider SDK. */
export function WorkspaceShell(props:{toolbar:any;content:any;auxiliary:any;status:any;auxiliaryOpen:boolean;wide:boolean;onLayout:(event:any)=>void}) {
 return <View onLayout={props.onLayout} style={{flex:1,backgroundColor:color("surface-app")}}>
  <View style={{paddingHorizontal:16,paddingVertical:12,gap:8,borderBottomWidth:1,borderColor:color("border-subtle")}}>{props.toolbar}</View>
  <View style={{flex:1,flexDirection:"row",position:"relative"}}>
   <View style={{flex:1,minWidth:0,padding:props.wide?24:12}}>{props.content}</View>
   <If when={props.auxiliaryOpen}><View style={{position:props.wide?"relative":"absolute",...(props.wide?{width:336}:{left:0,right:0,top:0,bottom:0}),backgroundColor:color("surface-content"),borderLeftWidth:props.wide?1:0,borderColor:color("border-subtle")}}>{props.auxiliary}</View></If>
  </View>
  <View style={{minHeight:28,paddingHorizontal:24,paddingVertical:4}}>{props.status}</View>
 </View>;
}
