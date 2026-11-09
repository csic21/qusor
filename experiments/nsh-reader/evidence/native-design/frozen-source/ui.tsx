import { Canvas, Path2D, Text, View, token, type JSX } from "nsh";
import { Button, Tab, Tabs } from "nsh/base";

export const ink = (role: string) => token(role as any);
export const fonts = { caption: 12, small: 13, ui: 14, heading: 17, body: 16, title: 26 };
export const line = { caption: 18, ui: 21, body: 28, title: 36 };

// Original, deliberately small vector vocabulary, painted by NSH Canvas.
const paths: Record<string, string> = {
  search: "M10.5 3.5a7 7 0 1 0 0 14a7 7 0 1 0 0-14 M16 16l4.5 4.5",
  plus: "M12 5v14 M5 12h14",
  close: "M6 6l12 12 M18 6L6 18",
  chevron: "M7 10l5 5l5-5",
  back: "M14 6l-6 6l6 6",
  document: "M5 3h10l4 4v14H5z M14 3v5h5 M8 12h8 M8 16h8",
  book: "M12 5v15 M3 4q5-2 9 1q4-3 9-1v15q-5-2-9 1q-4-3-9-1z",
  pen: "M5 16L16 5l3 3L8 19l-4 1z M14 7l3 3",
  plug: "M8 3v5 M16 3v5 M6 8h12v4a6 6 0 0 1-12 0z M12 18v4",
  code: "M8 6l-6 6l6 6 M16 6l6 6l-6 6 M14 3l-4 18",
  panel: "M3 4h18v16H3z M15 4v16",
  flow: "M3 3h7v6H3z M14 15h7v6h-7z M7 9v9h7 M7 13h11V7 M15 7h6",
  sun: "M12 7a5 5 0 1 0 0 10a5 5 0 1 0 0-10 M12 1v2 M12 21v2 M1 12h2 M21 12h2 M4 4l2 2 M18 18l2 2 M4 20l2-2 M18 6l2-2",
  moon: "M19 16A9 9 0 1 1 8 4A8 8 0 0 0 19 16z",
  more: "M5 11v2 M12 11v2 M19 11v2",
  arrow: "M5 12h14 M13 6l6 6l-6 6",
  check: "M5 12l5 5L20 7",
  stack: "M3 8l9-5l9 5l-9 5z M3 12l9 5l9-5 M3 16l9 5l9-5",
  circle: "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18",
};

export function Glyph(props: { name: string; size?: number; color?: string }) {
  return <Canvas style={{ width: props.size ?? 18, height: props.size ?? 18, pointerEvents: "none" }} draw={(cx, { width }) => {
    cx.scale(width / 24, width / 24);
    cx.strokeStyle = props.color ?? ink("text-secondary");
    cx.lineWidth = 1.7;
    cx.lineCap = "round";
    cx.lineJoin = "round";
    cx.stroke(new Path2D(paths[props.name] ?? paths.circle));
  }} />;
}

export function Mark(props: { size?: number }) {
  return <Canvas style={{ width: props.size ?? 25, height: props.size ?? 25 }} draw={(cx, { width }) => {
    cx.scale(width / 28, width / 28);
    cx.fillStyle = ink("text-primary");
    cx.beginPath(); cx.roundRect(3, 3, 22, 22, 6); cx.fill();
    cx.strokeStyle = ink("surface-paper"); cx.lineWidth = 1.5; cx.lineJoin = "round";
    cx.stroke(new Path2D("M8 9h12v10H8z M11 6v16 M8 13h12"));
  }} />;
}

export function Label(props: { children: JSX.Element; muted?: boolean; small?: boolean; weight?: "400" | "500" | "600" }) {
  return <Text style={{ color: ink(props.muted ? "text-secondary" : "text-primary"), fontSize: props.small ? fonts.small : fonts.ui, lineHeight: line.ui, fontWeight: props.weight ?? "400", numberOfLines: 1 }}>{props.children}</Text>;
}

export function Rule() { return <View style={{ height: 1, flexShrink: 0, backgroundColor: ink("border-subtle") }} />; }

export function Control(props: { label: string; icon?: string; text?: string; onPress?: () => void; selected?: boolean; raised?: boolean; primary?: boolean; children?: JSX.Element }) {
  return <Button accessibilityLabel={props.label} onPress={props.onPress}
    style={{ height: 30, minWidth: 30, paddingHorizontal: props.text ? 10 : 6, flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 6, borderRadius: 7, borderCurve: "continuous", borderWidth: props.raised || props.primary ? 1 : 0, borderColor: ink(props.primary ? "control-primary" : "border-control"), backgroundColor: ink(props.primary ? "control-primary" : props.selected ? "surface-selected" : props.raised ? "surface-control" : "transparent"), boxShadow: props.raised ? "0 1px 1px rgba(0,0,0,0.06), inset 0 1px 0 rgba(255,255,255,0.12)" : "none" }}
    hoverStyle={{ backgroundColor: ink(props.primary ? "control-primary-hover" : "surface-hover") }}
    activeStyle={{ backgroundColor: ink(props.primary ? "control-primary-pressed" : "surface-pressed"), boxShadow: "inset 0 1px 2px rgba(0,0,0,0.14)" }}
    focusStyle={{ outlineWidth: 2, outlineOffset: 2, outlineColor: ink("focus-ring") }}>
    <If when={props.icon}><Glyph name={props.icon!} size={16} color={ink(props.primary ? "control-primary-text" : props.selected ? "text-accent" : "text-secondary")} /></If>
    <If when={props.text}><Text style={{ fontSize: fonts.small, fontWeight: "500", color: ink(props.primary ? "control-primary-text" : "text-primary") }}>{props.text}</Text></If>
    {props.children}
  </Button>;
}

export function Segments(props: { value: string; onChange: (value: string) => void; items: { id: string; label: string }[] }) {
  return <Tabs style={{ flexDirection: "row", height: 31, padding: 2, borderRadius: 7, borderCurve: "continuous", borderWidth: 1, borderColor: ink("border-control"), backgroundColor: ink("surface-track"), boxShadow: "inset 0 1px 1px rgba(0,0,0,0.035)" }} indicatorStyle={{ top: 2, bottom: 2, borderRadius: 5, backgroundColor: ink("surface-control"), boxShadow: "0 1px 3px rgba(0,0,0,0.12)" }}>
    <Each of={props.items}>{(item) => <Tab accessibilityLabel={item().label} selected={props.value === item().id} onPress={() => props.onChange(item().id)} style={{ paddingHorizontal: 17, justifyContent: "center", alignItems: "center", borderRadius: 5 }} focusStyle={{ outlineWidth: 2, outlineColor: ink("focus-ring") }}><Text style={{ fontSize: fonts.small, fontWeight: props.value === item().id ? "600" : "400", color: ink(props.value === item().id ? "text-primary" : "text-secondary") }}>{item().label}</Text></Tab>}</Each>
  </Tabs>;
}
