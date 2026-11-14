import { Scroll, Text, View, type JSX } from "nsh";
import { Button, Input, Progress, Select, Switch, Tab, Tabs } from "nsh/base";
import { Copy, focus, Glyph, ink, Rule } from "../ui";
import {
    type Align,
    type ControlledSurfaceKind,
    type NodeStyle,
    PLUGIN_UI_PROTOCOL,
    type SemanticToken,
    type SpacingScale,
    type TypographyRole,
    type UiAction,
    type UiEventEnvelope,
    type UiNode,
    type UiTree,
} from "./ui_types";

const SEMANTIC_TOKEN_MAP: Record<SemanticToken, string> = {
    surface_base: "surface-base",
    surface_toolbar: "surface-toolbar",
    surface_inspector: "surface-inspector",
    surface_control: "surface-control",
    surface_selected: "surface-selected",
    surface_hover: "surface-hover",
    surface_pressed: "surface-pressed",
    surface_inset: "surface-inset",
    surface_accent: "surface-accent",
    surface_danger: "surface-danger",
    border_subtle: "border-subtle",
    border_control: "border-control",
    border_danger: "border-danger",
    text_primary: "text-primary",
    text_secondary: "text-secondary",
    text_accent: "text-accent",
    text_danger: "text-danger",
    focus_ring: "focus-ring",
    primary: "primary",
    primary_foreground: "primary-foreground",
    muted: "muted",
    muted_foreground: "muted-foreground",
};

const SPACING_PX: Record<SpacingScale, number> = {
    none: 0,
    xs: 4,
    sm: 8,
    md: 12,
    lg: 16,
    xl: 24,
};

const TYPOGRAPHY_STYLE: Record<
    TypographyRole,
    { fontSize: number; lineHeight: number; fontWeight: "400" | "600" }
> = {
    caption: { fontSize: 11, lineHeight: 16, fontWeight: "400" },
    body: { fontSize: 13, lineHeight: 20, fontWeight: "400" },
    body_strong: { fontSize: 13, lineHeight: 20, fontWeight: "600" },
    subtitle: { fontSize: 15, lineHeight: 22, fontWeight: "600" },
    title: { fontSize: 20, lineHeight: 28, fontWeight: "600" },
    mono: { fontSize: 12, lineHeight: 18, fontWeight: "400" },
};

const ALIGN_MAP: Record<
    Align,
    "flex-start" | "center" | "flex-end" | "stretch"
> = {
    start: "flex-start",
    center: "center",
    end: "flex-end",
    stretch: "stretch",
};

const ALLOWED_ICON_NAMES = new Set<string>([
    "search",
    "speaker",
    "plus",
    "close",
    "chevron",
    "back",
    "forward",
    "list",
    "document",
    "book",
    "pen",
    "plug",
    "panel",
    "info",
    "clock",
    "flow",
    "sun",
    "moon",
    "more",
    "arrow",
    "chat",
    "grid",
    "folder",
    "trash",
    "edit",
    "download",
    "upload",
    "bookmark",
    "sliders",
]);

function resolveToken(tokenName: SemanticToken | undefined, fallback: string): string {
    if (!tokenName) return ink(fallback);
    const mapped = SEMANTIC_TOKEN_MAP[tokenName];
    return ink(mapped ?? fallback);
}

function spacingValue(scale: SpacingScale | undefined, fallback = 0): number {
    if (!scale) return fallback;
    return SPACING_PX[scale] ?? fallback;
}

function containerStyle(style: NodeStyle | undefined) {
    return {
        backgroundColor: style?.background
            ? resolveToken(style.background, "transparent")
            : undefined,
        borderColor: style?.border
            ? resolveToken(style.border, "border-subtle")
            : undefined,
        borderWidth: style?.border ? 1 : undefined,
        padding: style?.padding ? spacingValue(style.padding) : undefined,
        gap: style?.gap ? spacingValue(style.gap) : undefined,
        alignItems: style?.align ? ALIGN_MAP[style.align] : undefined,
        flex: style?.flex_grow ? 1 : undefined,
    };
}

function parseTree(input: string | UiTree): UiTree | null {
    let parsed: unknown = input;
    if (typeof input === "string") {
        try {
            parsed = JSON.parse(input);
        } catch {
            return null;
        }
    }
    if (
        !parsed ||
        typeof parsed !== "object" ||
        (parsed as UiTree).protocol_version !== PLUGIN_UI_PROTOCOL ||
        typeof (parsed as UiTree).revision !== "number" ||
        !(parsed as UiTree).root ||
        typeof (parsed as UiTree).root.id !== "string" ||
        !(parsed as UiTree).root.component ||
        typeof (parsed as UiTree).root.component.kind !== "string"
    ) {
        return null;
    }
    return parsed as UiTree;
}

export interface DeclarativeUiRendererProps {
    /** Serialized `UiTree` JSON string or validated `UiTree` object from the host. */
    tree: string | UiTree;
    /** Active host slot mount identifier. */
    mountId: string;
    /** Active host generation fence counter. */
    generation: number;
    /** Compact/narrow layout hint from the host workspace shell. */
    compact?: boolean;
    /** Emits generation-fenced `UiEventEnvelope` events back to the host. */
    onEvent: (envelope: UiEventEnvelope) => void;
    /** Optional host broker callback for rendering `controlled_surface_ref` nodes. */
    renderControlledSurface?: (
        surfaceId: string,
        surfaceKind: ControlledSurfaceKind,
        node: UiNode,
    ) => JSX.Element;
}

/**
 * Generic NSH native renderer for Candidate A declarative plugin UI trees.
 *
 * Strictly bound to the whitelisted `UiComponent` vocabulary and `SemanticToken`
 * palette; contains no plugin-specific logic, raw CSS, HTML, or WebView paths.
 */
export function DeclarativeUiRenderer(props: DeclarativeUiRendererProps): JSX.Element {
    const parsed = parseTree(props.tree);
    if (!parsed) {
        return (
            <View
                accessibilityLabel="无法渲染插件界面"
                style={{
                    padding: 16,
                    borderRadius: 10,
                    borderWidth: 1,
                    borderColor: ink("border-danger"),
                    backgroundColor: ink("surface-danger"),
                    gap: 4,
                }}
            >
                <Copy strong>界面描述无效</Copy>
                <Copy size={12} muted>
                    宿主已拦截不符合声明式协议的界面数据。
                </Copy>
            </View>
        );
    }

    const emit = (nodeId: string, actionId: string, action: UiAction) => {
        props.onEvent({
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: props.mountId,
            generation: props.generation,
            node_id: nodeId,
            action_id: actionId,
            action,
        });
    };

    return renderNode(parsed.root, {
        compact: props.compact === true,
        emit,
        renderControlledSurface: props.renderControlledSurface,
    });
}

type RenderContext = {
    compact: boolean;
    emit: (nodeId: string, actionId: string, action: UiAction) => void;
    renderControlledSurface?: DeclarativeUiRendererProps["renderControlledSurface"];
};

function renderChildren(node: UiNode, ctx: RenderContext): JSX.Element[] {
    if (!node.children || node.children.length === 0) return [];
    const elements: JSX.Element[] = [];
    for (const child of node.children) {
        if (ctx.compact && child.style?.compact_hide) {
            continue;
        }
        elements.push(renderNode(child, ctx));
    }
    return elements;
}

function renderNode(node: UiNode, ctx: RenderContext): JSX.Element {
    if (ctx.compact && node.style?.compact_hide) {
        return <View style={{ display: "none" }} />;
    }

    const style = node.style;
    const a11y = node.accessibility;
    const component = node.component;
    const baseBox = containerStyle(style);

    switch (component.kind) {
        case "v_stack":
            return (
                <View
                    accessibilityLabel={a11y?.label}
                    accessibilityHint={a11y?.hint}
                    style={{
                        ...baseBox,
                        flexDirection: "column",
                        gap: spacingValue(style?.gap, 8),
                    }}
                >
                    {renderChildren(node, ctx)}
                </View>
            );

        case "h_stack":
            return (
                <View
                    accessibilityLabel={a11y?.label}
                    accessibilityHint={a11y?.hint}
                    style={{
                        ...baseBox,
                        flexDirection: "row",
                        alignItems: style?.align ? ALIGN_MAP[style.align] : "center",
                        gap: spacingValue(style?.gap, 8),
                    }}
                >
                    {renderChildren(node, ctx)}
                </View>
            );

        case "scroll":
            return (
                <Scroll
                    accessibilityLabel={a11y?.label}
                    style={{
                        ...baseBox,
                        flex: 1,
                        minHeight: 0,
                    }}
                >
                    <View style={{ gap: spacingValue(style?.gap, 8) }}>
                        {renderChildren(node, ctx)}
                    </View>
                </Scroll>
            );

        case "card":
            return (
                <View
                    accessibilityLabel={a11y?.label}
                    accessibilityHint={a11y?.hint}
                    style={{
                        borderRadius: 10,
                        borderCurve: "continuous",
                        borderWidth: 1,
                        borderColor: resolveToken(style?.border, "border-subtle"),
                        backgroundColor: resolveToken(style?.background, "surface-base"),
                        padding: spacingValue(style?.padding, 12),
                        gap: spacingValue(style?.gap, 8),
                        flex: style?.flex_grow ? 1 : undefined,
                    }}
                >
                    {renderChildren(node, ctx)}
                </View>
            );

        case "section":
            return (
                <View
                    accessibilityLabel={a11y?.label ?? component.title}
                    style={{
                        ...baseBox,
                        flexDirection: "column",
                        gap: spacingValue(style?.gap, 8),
                    }}
                >
                    <Text
                        style={{
                            fontSize: 12,
                            lineHeight: 18,
                            fontWeight: "600",
                            color: resolveToken(style?.foreground, "text-secondary"),
                        }}
                    >
                        {component.title}
                    </Text>
                    {renderChildren(node, ctx)}
                </View>
            );

        case "split": {
            const ratio = Math.min(90, Math.max(10, component.ratio_percent));
            const children = node.children ?? [];
            const first = children[0];
            const second = children[1];
            return (
                <View
                    accessibilityLabel={a11y?.label}
                    style={{
                        flexDirection: ctx.compact ? "column" : "row",
                        gap: spacingValue(style?.gap, 12),
                        flex: style?.flex_grow ? 1 : undefined,
                        minHeight: 0,
                    }}
                >
                    {first ? (
                        <View
                            style={
                                ctx.compact
                                    ? { minWidth: 0 }
                                    : { flexGrow: ratio, flexBasis: 0, minWidth: 0 }
                            }
                        >
                            {renderNode(first, ctx)}
                        </View>
                    ) : null}
                    {second ? (
                        <View
                            style={
                                ctx.compact
                                    ? { minWidth: 0 }
                                    : { flexGrow: 100 - ratio, flexBasis: 0, minWidth: 0 }
                            }
                        >
                            {renderNode(second, ctx)}
                        </View>
                    ) : null}
                </View>
            );
        }

        case "list":
            return (
                <View
                    role="list"
                    accessibilityLabel={a11y?.label}
                    style={{
                        ...baseBox,
                        flexDirection: "column",
                        gap: spacingValue(style?.gap, 4),
                    }}
                >
                    {renderChildren(node, ctx)}
                </View>
            );

        case "list_item": {
            const selected = component.selected === true;
            const itemStyle = {
                paddingHorizontal: 10,
                paddingVertical: 8,
                borderRadius: 8,
                borderCurve: "continuous" as const,
                backgroundColor: selected
                    ? ink("surface-selected")
                    : resolveToken(style?.background, "transparent"),
                gap: 4,
            };
            const body = (
                <View style={{ gap: 2, minWidth: 0 }}>
                    <Text
                        style={{
                            fontSize: 13,
                            lineHeight: 20,
                            fontWeight: selected ? "600" : "400",
                            color: selected ? ink("text-accent") : ink("text-primary"),
                        }}
                    >
                        {component.title}
                    </Text>
                    {component.subtitle ? (
                        <Text
                            style={{
                                fontSize: 12,
                                lineHeight: 18,
                                color: ink("text-secondary"),
                            }}
                        >
                            {component.subtitle}
                        </Text>
                    ) : null}
                    {renderChildren(node, ctx)}
                </View>
            );
            if (component.action_id) {
                const actionId = component.action_id;
                return (
                    <Button
                        role="button"
                        selected={selected}
                        accessibilityLabel={a11y?.label ?? component.title}
                        onPress={() => ctx.emit(node.id, actionId, { kind: "press" })}
                        style={itemStyle}
                        hoverStyle={{ backgroundColor: ink("surface-hover") }}
                        activeStyle={{ backgroundColor: ink("surface-pressed") }}
                        focusStyle={focus()}
                    >
                        {body}
                    </Button>
                );
            }
            return (
                <View
                    role="group"
                    accessibilityLabel={a11y?.label ?? component.title}
                    style={itemStyle}
                >
                    {body}
                </View>
            );
        }

        case "text": {
            const typo = TYPOGRAPHY_STYLE[style?.typography ?? "body"];
            return (
                <Text
                    accessibilityLabel={a11y?.label}
                    style={{
                        fontSize: typo.fontSize,
                        lineHeight: typo.lineHeight,
                        fontWeight: typo.fontWeight,
                        color: resolveToken(style?.foreground, "text-primary"),
                    }}
                >
                    {component.content}
                </Text>
            );
        }

        case "badge":
            return (
                <View
                    accessibilityLabel={a11y?.label ?? component.label}
                    style={{
                        paddingHorizontal: 7,
                        paddingVertical: 2,
                        borderRadius: 6,
                        borderCurve: "continuous",
                        backgroundColor: resolveToken(component.tone, "surface-inset"),
                        alignSelf: "flex-start",
                    }}
                >
                    <Text
                        style={{
                            fontSize: 11,
                            lineHeight: 16,
                            fontWeight: "600",
                            color: resolveToken(style?.foreground, "text-primary"),
                        }}
                    >
                        {component.label}
                    </Text>
                </View>
            );

        case "divider":
            return <Rule />;

        case "empty_state":
            return (
                <View
                    accessibilityLabel={a11y?.label ?? component.title}
                    style={{
                        padding: 20,
                        borderRadius: 10,
                        borderWidth: 1,
                        borderColor: ink("border-subtle"),
                        backgroundColor: ink("surface-inset"),
                        alignItems: "center",
                        gap: 6,
                    }}
                >
                    <Copy size={14} strong>
                        {component.title}
                    </Copy>
                    <Copy size={12} muted lines={3}>
                        {component.description}
                    </Copy>
                </View>
            );

        case "key_value":
            return (
                <View
                    accessibilityLabel={a11y?.label ?? `${component.key}: ${component.value}`}
                    style={{
                        flexDirection: "row",
                        alignItems: "center",
                        justifyContent: "space-between",
                        gap: 10,
                    }}
                >
                    <Copy size={12} muted>
                        {component.key}
                    </Copy>
                    <Copy size={12} strong>
                        {component.value}
                    </Copy>
                </View>
            );

        case "progress": {
            const total = Math.max(1, component.total);
            const percent = Math.min(100, Math.max(0, (component.completed / total) * 100));
            return (
                <Progress
                    value={percent}
                    accessibilityLabel={
                        a11y?.label ?? `${component.completed}/${component.total}`
                    }
                    style={{ flexDirection: "column", gap: 4 }}
                    trackStyle={{
                        height: 6,
                        borderRadius: 3,
                        backgroundColor: ink("surface-inset"),
                    }}
                    indicatorStyle={{
                        height: 6,
                        borderRadius: 3,
                        backgroundColor: resolveToken(style?.foreground, "primary"),
                    }}
                />
            );
        }

        case "button": {
            const disabled = component.disabled === true;
            const primary = component.primary === true;
            const destructive = component.destructive === true;
            const bgToken: SemanticToken = destructive
                ? "surface_danger"
                : primary
                  ? "primary"
                  : "surface_control";
            const fgToken: SemanticToken = destructive
                ? "text_danger"
                : primary
                  ? "primary_foreground"
                  : "text_primary";
            const borderToken: SemanticToken = destructive ? "border_danger" : "border_control";
            return (
                <Button
                    disabled={disabled}
                    accessibilityLabel={a11y?.label ?? component.label}
                    onPress={() => ctx.emit(node.id, component.action_id, { kind: "press" })}
                    style={{
                        height: 30,
                        paddingHorizontal: 12,
                        borderRadius: 7,
                        borderCurve: "continuous",
                        alignItems: "center",
                        justifyContent: "center",
                        backgroundColor: resolveToken(bgToken, "surface-control"),
                        borderWidth: primary ? 0 : 1,
                        borderColor: resolveToken(borderToken, "border-control"),
                        opacity: disabled ? 0.45 : 1,
                    }}
                    hoverStyle={{ backgroundColor: ink("surface-hover") }}
                    activeStyle={{ backgroundColor: ink("surface-pressed") }}
                    focusStyle={focus()}
                >
                    <Text
                        style={{
                            fontSize: 13,
                            lineHeight: 18,
                            fontWeight: primary ? "600" : "400",
                            color: resolveToken(fgToken, "text-primary"),
                        }}
                    >
                        {component.label}
                    </Text>
                </Button>
            );
        }

        case "icon_button": {
            const disabled = component.disabled === true;
            const iconName = ALLOWED_ICON_NAMES.has(component.icon_name)
                ? component.icon_name
                : "grid";
            return (
                <Button
                    disabled={disabled}
                    accessibilityLabel={a11y?.label ?? iconName}
                    onPress={() => ctx.emit(node.id, component.action_id, { kind: "press" })}
                    style={{
                        height: 30,
                        width: 30,
                        borderRadius: 7,
                        borderCurve: "continuous",
                        alignItems: "center",
                        justifyContent: "center",
                        backgroundColor: resolveToken(style?.background, "transparent"),
                        opacity: disabled ? 0.45 : 1,
                    }}
                    hoverStyle={{ backgroundColor: ink("surface-hover") }}
                    activeStyle={{ backgroundColor: ink("surface-pressed") }}
                    focusStyle={focus()}
                >
                    <Glyph name={iconName} size={16} />
                </Button>
            );
        }

        case "text_input": {
            const disabled = component.disabled === true;
            return (
                <Input
                    value={component.value}
                    placeholder={component.placeholder}
                    multiline={component.multiline === true}
                    readonly={disabled}
                    accessibilityLabel={
                        a11y?.label ??
                        (component.placeholder.length > 0 ? component.placeholder : "输入框")
                    }
                    onChange={({ value }) =>
                        ctx.emit(node.id, component.on_change_action, {
                            kind: "text_changed",
                            value,
                        })
                    }
                    onSubmit={({ text }) => {
                        if (component.on_submit_action) {
                            ctx.emit(node.id, component.on_submit_action, {
                                kind: "text_submitted",
                                value: text,
                            });
                        }
                    }}
                    style={{
                        minHeight: component.multiline ? 96 : 32,
                        paddingHorizontal: 10,
                        paddingVertical: 6,
                        borderRadius: 7,
                        borderWidth: 1,
                        borderColor: ink("border-control"),
                        backgroundColor: ink("surface-control"),
                        color: ink("text-primary"),
                        fontSize: 13,
                        lineHeight: 20,
                        opacity: disabled ? 0.5 : 1,
                    }}
                    focusStyle={focus()}
                />
            );
        }

        case "toggle": {
            const disabled = component.disabled === true;
            return (
                <Switch
                    checked={component.checked}
                    disabled={disabled}
                    accessibilityLabel={a11y?.label ?? component.label}
                    onChange={({ checked }) =>
                        ctx.emit(node.id, component.action_id, {
                            kind: "toggled",
                            checked,
                        })
                    }
                    style={{
                        flexDirection: "row",
                        alignItems: "center",
                        gap: 8,
                        opacity: disabled ? 0.45 : 1,
                    }}
                    trackStyle={{
                        width: 34,
                        height: 20,
                        borderRadius: 10,
                        padding: 2,
                        backgroundColor: component.checked
                            ? ink("primary")
                            : ink("surface-inset"),
                        borderWidth: 1,
                        borderColor: component.checked
                            ? ink("primary")
                            : ink("border-control"),
                    }}
                    thumbStyle={{
                        width: 14,
                        height: 14,
                        borderRadius: 7,
                        backgroundColor: ink("primary-foreground"),
                    }}
                >
                    <Copy size={13}>{component.label}</Copy>
                </Switch>
            );
        }

        case "select": {
            const disabled = component.disabled === true;
            const activeLabel =
                component.options.find((opt) => opt.id === component.selected_id)?.label ??
                component.selected_id;
            return (
                <Select
                    items={component.options.map((opt) => ({
                        value: opt.id,
                        title: opt.label,
                    }))}
                    value={component.selected_id}
                    disabled={disabled}
                    accessibilityLabel={a11y?.label ?? activeLabel}
                    onChange={({ value }) => {
                        if (value !== null) {
                            ctx.emit(node.id, component.action_id, {
                                kind: "selected",
                                option_id: value,
                            });
                        }
                    }}
                    trigger={
                        <View
                            style={{
                                height: 30,
                                paddingHorizontal: 10,
                                borderRadius: 7,
                                borderWidth: 1,
                                borderColor: ink("border-control"),
                                backgroundColor: ink("surface-control"),
                                flexDirection: "row",
                                alignItems: "center",
                                gap: 6,
                                opacity: disabled ? 0.45 : 1,
                            }}
                        >
                            <Copy size={13}>{activeLabel}</Copy>
                            <Glyph name="chevron" size={13} />
                        </View>
                    }
                    popupStyle={{
                        padding: 4,
                        borderRadius: 8,
                        borderWidth: 1,
                        borderColor: ink("border-subtle"),
                        backgroundColor: ink("surface-floating"),
                    }}
                    itemStyle={{
                        paddingHorizontal: 8,
                        paddingVertical: 6,
                        borderRadius: 5,
                        fontSize: 13,
                        color: ink("text-primary"),
                    }}
                />
            );
        }

        case "tabs":
            return (
                <Tabs
                    style={{
                        flexDirection: "row",
                        padding: 2,
                        borderRadius: 7,
                        backgroundColor: ink("surface-inset"),
                        gap: 2,
                    }}
                    indicatorStyle={{
                        top: 2,
                        bottom: 2,
                        borderRadius: 5,
                        backgroundColor: ink("surface-active-tab"),
                    }}
                >
                    {component.items.map((item) => {
                        const selected = item.id === component.active_tab_id;
                        return (
                            <Tab
                                selected={selected}
                                accessibilityLabel={item.label}
                                onPress={() =>
                                    ctx.emit(node.id, component.action_id, {
                                        kind: "tab_changed",
                                        tab_id: item.id,
                                    })
                                }
                                style={{
                                    paddingHorizontal: 10,
                                    paddingVertical: 5,
                                    borderRadius: 5,
                                    alignItems: "center",
                                    justifyContent: "center",
                                }}
                                focusStyle={focus()}
                            >
                                <Copy size={12} strong={selected} muted={!selected}>
                                    {item.label}
                                </Copy>
                            </Tab>
                        );
                    })}
                </Tabs>
            );

        case "controlled_surface_ref":
            if (ctx.renderControlledSurface) {
                return ctx.renderControlledSurface(
                    component.surface_id,
                    component.surface_kind,
                    node,
                );
            }
            return (
                <View
                    accessibilityLabel={a11y?.label ?? "受控原生表面"}
                    style={{
                        padding: 12,
                        borderRadius: 8,
                        borderWidth: 1,
                        borderColor: ink("border-subtle"),
                        backgroundColor: ink("surface-inset"),
                        gap: 4,
                    }}
                >
                    <Copy size={12} strong>
                        受控原生表面
                    </Copy>
                    <Copy size={11} muted>
                        {component.surface_kind}
                    </Copy>
                </View>
            );

        default:
            return (
                <View
                    style={{
                        padding: 8,
                        borderRadius: 6,
                        borderWidth: 1,
                        borderColor: ink("border-danger"),
                        backgroundColor: ink("surface-danger"),
                    }}
                >
                    <Copy size={12} muted>
                        不支持的组件类型
                    </Copy>
                </View>
            );
    }
}
