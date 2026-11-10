import { Scroll, View } from "nsh";
import { ActionButton, Body, InlineStatus, QuoteBlock, color } from "./ui";

export type NoteSummary = {
  capture_id: number;
  commit_rev: number;
  operation_id: string;
  document_id: number;
  document_title: string;
  page_index: number;
  quote_preview: string;
  quote_bytes: number;
  quote_truncated: boolean;
  note_preview: string;
  note_bytes: number;
  note_truncated: boolean;
  compound: boolean;
};

export function NotesView(props: {
  notes: () => NoteSummary[];
  status: () => string;
  currentDocumentId: () => number | null;
  onOpen: (note: NoteSummary) => void;
  onRefresh: () => void;
  onClose: () => void;
}) {
  return <View style={{flex:1,gap:12,minHeight:0}}>
    <View style={{flexDirection:"row",alignItems:"center",gap:12}}>
      <View style={{flex:1,minWidth:0,gap:4}}>
        <Body title>想法</Body>
        <Body muted>想法保存在本机；列表只显示有界预览，原文与个人观点分开保留。</Body>
      </View>
      <ActionButton label="刷新" compact onPress={props.onRefresh}/>
      <ActionButton label="返回阅读" compact onPress={props.onClose}/>
    </View>
    <If when={!!props.status()}><InlineStatus>{props.status()}</InlineStatus></If>
    <If when={props.notes().length===0}>
      <View style={{flex:1,justifyContent:"center",alignItems:"flex-start",padding:24,gap:8}}>
        <Body title>还没有已保存的想法</Body>
        <Body muted>在正文中选文并写下想法，保存后会出现在这里。</Body>
      </View>
    </If>
    <If when={props.notes().length>0}>
      <Scroll style={{flex:1,minHeight:0}}>
        <View style={{gap:10,paddingBottom:24}}>
          <Each of={props.notes()} key={(note:NoteSummary)=>`${note.capture_id}:${note.commit_rev}`}>{(entry)=>{
            const note=entry();
            const current=props.currentDocumentId()===note.document_id;
            return <View style={{gap:10,padding:14,borderWidth:1,borderRadius:8,borderColor:color("border-subtle"),backgroundColor:color("surface-content")}}>
              <View style={{flexDirection:"row",alignItems:"center",gap:8}}>
                <View style={{flex:1,minWidth:0}}><Body title>{note.document_title}</Body></View>
                <Body muted>{`第 ${note.page_index+1} 页`}</Body>
              </View>
              <QuoteBlock text={note.quote_preview}/>
              <If when={note.quote_truncated}><InlineStatus>引文较长，列表只显示开头；原记录未被截断。</InlineStatus></If>
              <View style={{gap:4}}>
                <Body muted>我的想法</Body>
                <Body>{note.note_preview||"（这条记录没有个人想法）"}</Body>
                <If when={note.note_truncated}><InlineStatus>想法较长，列表只显示开头；完整内容仍保存在本机。</InlineStatus></If>
              </View>
              <View style={{flexDirection:"row",alignItems:"center",gap:8}}>
                <If when={current}><ActionButton label="打开所在页" compact onPress={()=>props.onOpen(note)}/></If>
                <If when={!current}><InlineStatus>先打开《{note.document_title}》后可定位到这一页。</InlineStatus></If>
              </View>
            </View>;
          }}</Each>
        </View>
      </Scroll>
    </If>
  </View>;
}
