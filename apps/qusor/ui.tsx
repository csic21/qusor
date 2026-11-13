import { Canvas, Path2D, Text, View, token, type JSX } from "nsh";
import { Button, Dialog } from "nsh/base";

export const ink = (role: string) => token(role as any);
const paths: Record<string, string> = {
    search: "M10.5 3.5a7 7 0 1 0 0 14a7 7 0 1 0 0-14 M16 16l4.5 4.5",
    speaker: "M11 5L6 9H3v6h3l5 4z M15.5 8.5a5 5 0 0 1 0 7 M18.5 6a8.5 8.5 0 0 1 0 12",
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
    trash: "M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13 M10 11v6 M14 11v6",
    edit: "M4 20h4L18.5 9.5a1.5 1.5 0 0 0-4-4L4 16z M13.5 6.5l4 4",
    download: "M12 4v11 M7 11l5 5l5-5 M5 19h14",
    upload: "M12 16V5 M7 9l5-5l5 5 M5 19h14",
    bookmark: "M6 4h12v17l-6-4l-6 4z",
    sliders: "M4 7h16 M4 12h16 M4 17h16 M8 5v4 M15 10v4 M11 15v4",
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

export type ControlSize = "compact" | "regular" | "large" | "touch";

/// One size scale for every button and text action. Height, padding, radius,
/// icon and label sizes move together so a row of controls reads as one system.
export const control = (size: ControlSize = "regular") => {
    switch (size) {
        case "compact":
            return {
                height: 28,
                padding: 9,
                radius: 6,
                gap: 6,
                icon: 15,
                text: 12,
            };
        case "large":
            return {
                height: 34,
                padding: 14,
                radius: 8,
                gap: 8,
                icon: 17,
                text: 13,
            };
        case "touch":
            return {
                height: 44,
                padding: 16,
                radius: 10,
                gap: 8,
                icon: 20,
                text: 15,
            };
        default:
            return {
                height: 30,
                padding: 10,
                radius: 7,
                gap: 7,
                icon: 16,
                text: 13,
            };
    }
};

/// A text-only action with the same height, padding, radius, hover, active and
/// focus treatment as `Tool`. `filled` is the quieter inset surface used for
/// inline actions; a plain button stays transparent.
export function TextButton(props: {
    label: string;
    accessibilityLabel?: string;
    onPress?: () => void;
    disabled?: boolean;
    filled?: boolean;
    raised?: boolean;
    size?: ControlSize;
}) {
    const size = control(props.size);
    return (
        <Button
            accessibilityLabel={props.accessibilityLabel ?? props.label}
            disabled={props.disabled}
            onPress={props.onPress}
            style={{
                height: size.height,
                paddingHorizontal: size.padding,
                alignItems: "center",
                justifyContent: "center",
                borderRadius: size.radius,
                borderCurve: "continuous",
                backgroundColor: ink(
                    props.raised
                        ? "surface-control"
                        : props.filled
                          ? "surface-inset"
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
            <Copy size={size.text}>{props.label}</Copy>
        </Button>
    );
}

/// Shared menu geometry: items keep one height and padding, and the popup
/// keeps one width floor and padding across every context menu.
export const menuPopup = () => ({
    padding: 5,
    minWidth: 200,
    borderRadius: 10,
    backgroundColor: ink("surface-floating"),
    borderWidth: 1,
    borderColor: ink("border-subtle"),
    boxShadow: "0 8px 28px rgba(0,0,0,0.16)",
});

export const menuItem = () => ({
    height: 30,
    paddingHorizontal: 10,
    borderRadius: 6,
    flexDirection: "row" as const,
    alignItems: "center" as const,
    gap: 8,
});
export function Tool(props: {
    label: string;
    icon?: string;
    text?: string;
    selected?: boolean;
    raised?: boolean;
    onPress?: () => void;
    disabled?: boolean;
    size?: ControlSize;
}) {
    const size = control(props.size);
    return (
        <Button
            accessibilityLabel={props.label}
            disabled={props.disabled}
            onPress={props.onPress}
            style={{
                height: size.height,
                minWidth: size.height,
                paddingHorizontal: props.text ? size.padding : 6,
                flexDirection: "row",
                alignItems: "center",
                justifyContent: "center",
                gap: size.gap,
                borderRadius: size.radius,
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
                    size={size.icon}
                    color={ink(
                        props.selected ? "text-accent" : "text-secondary",
                    )}
                />
            </If>
            <If when={props.text}>
                <Copy size={size.text}>{props.text}</Copy>
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

export interface BottomSheetProps {
    open: boolean;
    onClose: () => void;
    title?: string;
    subtitle?: string;
    headerRight?: JSX.Element;
    maxHeight?: number | `${number}%`;
    children?: JSX.Element;
    testID?: string;
}

/**
 * Mobile bottom sheet built on NSH `Dialog`, respecting bottom safe area and
 * software keyboard inset heights.
 */
export function BottomSheet(props: BottomSheetProps) {
    return (
        <Dialog
            testID={props.testID}
            open={props.open}
            onOpenChange={({ open }) => {
                if (!open) props.onClose();
            }}
            animation="slide"
            backdropStyle={{ backgroundColor: ink("surface-scrim") }}
            style={{
                position: "absolute",
                left: 0,
                right: 0,
                bottom: 0,
                maxHeight: props.maxHeight ?? "85%",
                marginBottom: "env(keyboard-inset-height)",
                paddingTop: 10,
                paddingBottom: "calc(env(safe-area-inset-bottom) + 16px)",
                paddingLeft: "calc(env(safe-area-inset-left) + 16px)",
                paddingRight: "calc(env(safe-area-inset-right) + 16px)",
                borderTopLeftRadius: 18,
                borderTopRightRadius: 18,
                borderCurve: "continuous",
                backgroundColor: ink("surface-floating"),
                borderTopWidth: 1,
                borderLeftWidth: 1,
                borderRightWidth: 1,
                borderColor: ink("border-control"),
                boxShadow: "0 -12px 40px rgba(0,0,0,0.20)",
                gap: 12,
            }}
        >
            <View
                style={{
                    alignSelf: "center",
                    width: 36,
                    height: 4,
                    borderRadius: 2,
                    backgroundColor: ink("border-control"),
                }}
            />
            <If when={props.title || props.headerRight}>
                <View
                    style={{
                        flexDirection: "row",
                        alignItems: "center",
                        gap: 10,
                        minHeight: 36,
                    }}
                >
                    <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                        <If when={props.title}>
                            <Copy size={16} strong>
                                {props.title ?? ""}
                            </Copy>
                        </If>
                        <If when={props.subtitle}>
                            <Copy size={12} muted>
                                {props.subtitle ?? ""}
                            </Copy>
                        </If>
                    </View>
                    <If when={props.headerRight}>{props.headerRight}</If>
                    <Tool
                        label="关闭"
                        icon="close"
                        size="touch"
                        onPress={props.onClose}
                    />
                </View>
                <Rule />
            </If>
            <View style={{ flexShrink: 1, minHeight: 0 }}>{props.children}</View>
        </Dialog>
    );
}
