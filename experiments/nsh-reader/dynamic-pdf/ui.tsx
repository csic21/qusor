import { Text, View, token } from "nsh";
import { Button, Input } from "nsh/base";
export const space = { small: 8, medium: 16, section: 24 } as const;
export const color = (role: string): string => token(role as any) as string;
export function ActionButton(props: { label: string; onPress: () => void; disabled?: boolean; primary?: boolean; compact?: boolean; buttonRef?: (node: any) => void }) {
 return <Button ref={props.buttonRef} disabled={!!props.disabled} onPress={props.onPress} accessibilityLabel={props.label}
  style={{minHeight:props.compact?36:44,paddingHorizontal:12,paddingVertical:8,borderRadius:6,borderWidth:2,borderColor:color("border-clear"),justifyContent:"center",backgroundColor:props.primary?color("action-background"):color("surface-app")}}
  hoverStyle={{backgroundColor:props.primary?color("action-hover"):color("selection-background")}}
  activeStyle={{backgroundColor:props.primary?color("action-pressed"):color("selection-background")}}
  focusStyle={{borderColor:color(props.primary?"action-foreground":"focus-ring"),...(props.primary?{backgroundColor:color("action-pressed")}:{})}}
  disabledStyle={{backgroundColor:color("action-disabled-background"),opacity:.7}}>
  <Text style={{fontSize:14,lineHeight:20,color:props.disabled?color("action-disabled-foreground"):props.primary?color("action-foreground"):color("text-primary")}}>{props.label}</Text>
 </Button>;
}
export function Body(props:{children:any; muted?:boolean; title?:boolean}) {
 return <Text style={{fontSize:props.title?16:14,lineHeight:props.title?24:22,fontWeight:props.title?"500":"400",color:color(props.muted?"text-secondary":"text-primary")}}>{props.children}</Text>;
}
export function InlineStatus(props:{children:any; danger?:boolean; success?:boolean}) {
 return <Text style={{fontSize:13,lineHeight:20,color:color(props.danger?"status-danger":props.success?"status-success":"text-secondary")}}>{props.children}</Text>;
}
export function QuoteBlock(props:{text:string}) {
 return <View style={{borderLeftWidth:3,borderColor:color("annotation-highlight"),paddingLeft:12,paddingVertical:4}}><Text style={{fontSize:16,lineHeight:25,color:color("text-primary")}}>{props.text}</Text></View>;
}
export function NoteEditor(props:{value:string; readonly:boolean; onChange:(value:string)=>void; inputRef?:(node:any)=>void; onComposition?:(composing:boolean)=>void}) {
 return <View style={{gap:8}}><Body>我的想法</Body><Input ref={props.inputRef} accessibilityLabel="我的想法" value={props.value} onChange={({value}:any)=>props.onChange(value)} onCompositionChange={({composing})=>props.onComposition?.(composing)} readonly={props.readonly} multiline placeholder="写下自己的理解、疑问或联系…"
  style={{minHeight:180,fontSize:15,lineHeight:24,padding:12,borderWidth:1,borderRadius:6,borderColor:color("border-subtle"),backgroundColor:color("surface-content"),color:color("text-primary")}}
  focusStyle={{borderColor:color("focus-ring")}}/></View>;
}
