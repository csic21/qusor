import { Canvas, Path2D, Text, View, token, type JSX } from "nsh";
import { Button } from "nsh/base";

export const ink = (role: string) => token(role as any);
const paths: Record<string, string> = {
    search: "M10.5 3.5a7 7 0 1 0 0 14a7 7 0 1 0 0-14 M16 16l4.5 4.5",
    plus: "M12 5v14 M5 12h14",
    close: "M6 6l12 12 M18 6L6 18",
    chevron: "M7 10l5 5l5-5",
    back: "M14 6l-6 6l6 6",
    forward: "M9 6l6 6l-6 6",
    list: "M4 5h2 M10 5h10 M4 12h2 M10 12h10 M4 19h2 M10 19h10",
    document: "M5 3h10l4 4v14H5z M14 3v5h5 M8 12h8 M8 16h6",
    book: "M12 5v15 M3 4q5-2 9 1q4-3 9-1v15q-5-2-9 1q-4-3-9-1z",
    pen: "M5 16L16 5l3 3L8 19l-4 1z M14 7l3 3",
    plug: "M8 3v5 M16 3v5 M6 8h12v4a6 6 0 0 1-12 0z M12 18v4",
    panel: "M3 4h18v16H3z M8 4v16",
    info: "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18 M12 11v6 M12 7v1",
    clock: "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18 M12 7v6h5",
    flow: "M3 3h7v6H3z M14 15h7v6h-7z M7 9v9h7 M7 13h11V7 M15 7h6",
    sun: "M12 7a5 5 0 1 0 0 10a5 5 0 1 0 0-10 M12 1v2 M12 21v2 M1 12h2 M21 12h2 M4 4l2 2 M18 18l2 2 M4 20l2-2 M18 6l2-2",
    moon: "M19 16A9 9 0 1 1 8 4A8 8 0 0 0 19 16z",
    more: "M5 11v2 M12 11v2 M19 11v2",
    arrow: "M5 12h14 M13 6l6 6l-6 6",
    chat: "M4 4h16v12H9l-5 4z M8 8h8 M8 12h5",
    grid: "M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z",
    folder: "M3 6h7l2 2h9v12H3z M3 6V4h7l2 2",
};
export function Glyph(props: { name: string; size?: number; color?: string }) {
    return (
        <Canvas
            style={{
                width: props.size ?? 18,
                height: props.size ?? 18,
                pointerEvents: "none",
                flexShrink: 0,
            }}
            draw={(cx, { width }) => {
                cx.scale(width / 24, width / 24);
                cx.strokeStyle = props.color ?? ink("text-secondary");
                cx.lineWidth = 1.6;
                cx.lineCap = "round";
                cx.lineJoin = "round";
                cx.stroke(new Path2D(paths[props.name] ?? paths.grid));
            }}
        />
    );
}
export function Copy(props: {
    children: JSX.Element;
    size?: number;
    muted?: boolean;
    strong?: boolean;
    lines?: number;
}) {
    return (
        <Text
            style={{
                fontSize: props.size ?? 13,
                lineHeight: (props.size ?? 13) + 7,
                fontWeight: props.strong ? "600" : "400",
                color: ink(props.muted ? "text-secondary" : "text-primary"),
                numberOfLines: props.lines ?? 1,
            }}
        >
            {props.children}
        </Text>
    );
}
export function Rule() {
    return (
        <View
            style={{
                height: 1,
                flexShrink: 0,
                backgroundColor: ink("border-subtle"),
            }}
        />
    );
}
export const focus = () => ({
    outlineWidth: 2,
    outlineOffset: 2,
    outlineColor: ink("focus-ring"),
});
export function Tool(props: {
    label: string;
    icon?: string;
    text?: string;
    selected?: boolean;
    raised?: boolean;
    onPress?: () => void;
    disabled?: boolean;
}) {
    return (
        <Button
            accessibilityLabel={props.label}
            disabled={props.disabled}
            onPress={props.onPress}
            style={{
                height: 30,
                minWidth: 30,
                paddingHorizontal: props.text ? 10 : 6,
                flexDirection: "row",
                alignItems: "center",
                justifyContent: "center",
                gap: 6,
                borderRadius: 7,
                borderCurve: "continuous",
                backgroundColor: ink(
                    props.selected
                        ? "surface-selected"
                        : props.raised
                          ? "surface-control"
                          : "transparent",
                ),
                borderWidth: props.raised ? 1 : 0,
                borderColor: ink("border-control"),
                opacity: props.disabled ? 0.4 : 1,
            }}
            hoverStyle={{ backgroundColor: ink("surface-hover") }}
            activeStyle={{ backgroundColor: ink("surface-pressed") }}
            focusStyle={focus()}
        >
            <If when={props.icon}>
                <Glyph
                    name={props.icon!}
                    size={17}
                    color={ink(
                        props.selected ? "text-accent" : "text-secondary",
                    )}
                />
            </If>
            <If when={props.text}>
                <Copy>{props.text}</Copy>
            </If>
        </Button>
    );
}
export function Badge(props: { icon: string; size?: number; tone?: string }) {
    return (
        <View
            style={{
                width: props.size ?? 34,
                height: props.size ?? 34,
                borderRadius: 8,
                borderCurve: "continuous",
                backgroundColor: ink(props.tone ?? "surface-inset"),
                justifyContent: "center",
                alignItems: "center",
                flexShrink: 0,
            }}
        >
            <Glyph
                name={props.icon}
                size={(props.size ?? 34) * 0.55}
                color={ink("text-accent")}
            />
        </View>
    );
}
export function ProviderIdentity(props: { title: string; icon: string }) {
    return (
        <View
            accessibilityLabel={`${props.title}插件提供的界面`}
            style={{
                flexDirection: "row",
                gap: 7,
                alignItems: "center",
                minWidth: 0,
            }}
        >
            <Glyph name={props.icon} size={16} />
            <Copy strong>{props.title}</Copy>
            <View
                style={{
                    paddingHorizontal: 5,
                    borderRadius: 4,
                    backgroundColor: ink("surface-inset"),
                }}
            >
                <Copy size={11} muted>
                    插件
                </Copy>
            </View>
        </View>
    );
}
