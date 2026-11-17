import {
    Canvas,
    effect,
    Image,
    Scroll,
    signal,
    Text,
    View,
    type JSX,
} from "nsh";
import { Input } from "nsh/base";
import { LibraryHost, PluginSlotHost } from "qusor/pdf-surface";
import { Badge, Copy, ink, Rule, Tool } from "../ui";
import {
    displayBoxes,
    fitPageSize,
    parseDocumentHandle,
    pdfHighlightBoxes,
    resolveQuoteRange,
    type DocumentFormat,
    type HighlightBox,
    type ParsedDocumentHandle,
    type Size,
} from "../document_surface";
import { type SavedSelectionRange } from "../reading_selection";
import { type ControlledSurfaceKind } from "./ui_types";

export interface ControlledDocumentSurfaceProps {
    mountId: string;
    surfaceId: string;
    surfaceKind: ControlledSurfaceKind;
    compact?: boolean;
    onCaptured?: () => void;
}

type PageMeta = {
    pageIndex: number;
    pageCount: number;
    width: number;
    height: number;
};

type SlotResult = {
    text?: unknown;
    next_offset?: unknown;
    ranges?: unknown;
};

const DEFAULT_DISPLAY: Size = { width: 480, height: 640 };
const DEFAULT_COMPACT_DISPLAY: Size = { width: 280, height: 380 };
const MAX_PAGE_SCALARS = 64 * 1024;
let nextSurfaceReq = 1;

function errorMessage(error: unknown, fallback: string): string {
    if (error instanceof Error && error.message) return error.message;
    const text = String(error ?? "").trim();
    return text || fallback;
}

function finitePositive(value: unknown): value is number {
    return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function safeNatural(value: unknown): value is number {
    return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function safePositive(value: unknown): value is number {
    return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function parsePageInfo(raw: string, handle: ParsedDocumentHandle): PageMeta | null {
    if (!raw || raw.length > 64 * 1024) return null;
    try {
        const value = JSON.parse(raw);
        if (
            value?.ok !== true ||
            value.document_id !== handle.documentId ||
            value.page_index !== handle.sectionIndex ||
            !finitePositive(value.width) ||
            !finitePositive(value.height) ||
            !safePositive(value.page_count) ||
            handle.sectionIndex >= value.page_count
        ) {
            return null;
        }
        return {
            pageIndex: value.page_index,
            pageCount: value.page_count,
            width: value.width,
            height: value.height,
        };
    } catch (_) {
        return null;
    }
}

function parseGeometrySize(raw: string): Size | null {
    if (!raw || raw.length > 512 * 1024) return null;
    try {
        const value = JSON.parse(raw);
        if (!finitePositive(value?.width) || !finitePositive(value?.height)) {
            return null;
        }
        return { width: value.width, height: value.height };
    } catch (_) {
        return null;
    }
}

function sanitizeRanges(raw: unknown): SavedSelectionRange[] {
    if (!Array.isArray(raw)) return [];
    const ranges: SavedSelectionRange[] = [];
    for (const value of raw) {
        if (
            !safePositive(value?.capture_id) ||
            !safeNatural(value?.start) ||
            !safeNatural(value?.end) ||
            value.start >= value.end
        ) {
            continue;
        }
        ranges.push({
            capture_id: value.capture_id,
            start: value.start,
            end: value.end,
        });
    }
    return ranges;
}

function callSlotKnowledge(mountId: string, operation: Record<string, unknown>): SlotResult {
    const requestId = `surface-req-${nextSurfaceReq++}`;
    const raw = PluginSlotHost.slotKnowledgeCall(
        mountId,
        JSON.stringify({
            protocol_version: 1,
            api_version: { major: 0, minor: 8 },
            request_id: requestId,
            operation,
        }),
    );
    const outer = JSON.parse(raw);
    if (!outer?.ok || !outer.value) {
        throw new Error(String(outer?.error ?? "正交知识协议调用失败"));
    }
    const result = outer.value.result;
    if (!result || result.kind === "error") {
        throw new Error(String(result?.error?.message ?? "正交知识操作返回错误"));
    }
    return result as SlotResult;
}

function formatLabel(format: DocumentFormat, sectionIndex: number): string {
    return `${format.toUpperCase()} 原生受控视口 · 第 ${sectionIndex + 1} ${format === "pdf" ? "页" : "章"}`;
}

export function ControlledDocumentSurface(
    props: ControlledDocumentSurfaceProps,
): JSX.Element {
    const parsed = parseDocumentHandle(props.surfaceId);
    const loading = signal(true);
    const error = signal("");
    const sectionText = signal("");
    const savedRanges = signal<SavedSelectionRange[]>([]);
    const pageUrl = signal("");
    const pageInfo = signal<PageMeta | null>(null);
    const pageDisplay = signal<Size | null>(null);
    const highlightBoxes = signal<HighlightBox[]>([]);
    const highlightMessage = signal("");
    const pageFailure = signal("");
    const quoteDraft = signal("");
    const noteDraft = signal("");
    const statusMsg = signal("");
    const saving = signal(false);

    let requestSeq = 0;
    let livePageUrl: string | null = null;

    const releasePageUrl = () => {
        const current = livePageUrl;
        livePageUrl = null;
        pageUrl.set("");
        if (current) URL.revokeObjectURL(current);
    };

    const isCurrent = (request: number) => request === requestSeq;

    const loadSectionText = (current: ParsedDocumentHandle): string => {
        let text = "";
        let offset = 0;
        for (let chunkIndex = 0; chunkIndex < 16; chunkIndex += 1) {
            const chunk = callSlotKnowledge(props.mountId, {
                kind: "document_chunk",
                format: current.format,
                document_id: current.documentId,
                section_index: current.sectionIndex,
                offset,
                max_bytes: 2048,
            });
            if (chunk.text !== undefined && typeof chunk.text !== "string") {
                throw new Error("当前章节文本超出受控读取上限。");
            }
            const chunkText = String(chunk.text ?? "");
            if (Array.from(text + chunkText).length > MAX_PAGE_SCALARS) {
                throw new Error("当前章节文本超出受控读取上限。");
            }
            text += chunkText;
            const next = chunk.next_offset;
            if (
                typeof next !== "number" ||
                !Number.isSafeInteger(next) ||
                next <= offset
            ) {
                break;
            }
            offset = next;
        }
        return text;
    };

    const loadPage = async (
        current: ParsedDocumentHandle,
        request: number,
    ): Promise<void> => {
        if (current.format !== "pdf") {
            pageFailure.set("");
            highlightMessage.set("EPUB 暂不支持原生几何高亮。");
            pageDisplay.set(props.compact ? DEFAULT_COMPACT_DISPLAY : DEFAULT_DISPLAY);
            return;
        }

        const rendered = await LibraryHost.pageImage(
            current.documentId,
            current.sectionIndex,
        );
        if (!isCurrent(request)) return;
        if (!rendered || !(rendered.file instanceof File)) {
            pageFailure.set(
                `第 ${current.sectionIndex + 1} 页的页图不可用；正文与已保存摘录仍可读取。`,
            );
            pageDisplay.set(props.compact ? DEFAULT_COMPACT_DISPLAY : DEFAULT_DISPLAY);
            return;
        }

        let objectUrl: string | null = null;
        try {
            const meta = parsePageInfo(String(rendered.info ?? ""), current);
            if (!meta) {
                throw new Error(
                    `第 ${current.sectionIndex + 1} 页的页图信息无效；正文与已保存摘录仍可读取。`,
                );
            }
            objectUrl = URL.createObjectURL(rendered.file);
            if (!isCurrent(request)) {
                URL.revokeObjectURL(objectUrl);
                return;
            }

            let boxes: HighlightBox[] = [];
            let display: Size;
            const highlight = pdfHighlightBoxes(
                String(rendered.geometry ?? ""),
                Boolean(props.compact),
                savedRanges(),
                current.mode === "focus" ? current.captureId : null,
            );
            if (highlight.ok) {
                const geometry = parseGeometrySize(String(rendered.geometry ?? ""));
                if (geometry) {
                    boxes = displayBoxes(highlight.boxes, geometry, highlight.display);
                    display = highlight.display;
                } else {
                    display = fitPageSize(meta.width, meta.height, Boolean(props.compact));
                    highlightMessage.set(
                        `第 ${current.sectionIndex + 1} 页：几何尺寸无效，已显示页图但无法精确高亮。`,
                    );
                }
            } else {
                display = fitPageSize(meta.width, meta.height, Boolean(props.compact));
                highlightMessage.set(
                    `第 ${current.sectionIndex + 1} 页：${highlight.message}`,
                );
            }

            if (!isCurrent(request)) {
                URL.revokeObjectURL(objectUrl);
                return;
            }
            releasePageUrl();
            livePageUrl = objectUrl;
            objectUrl = null;
            pageUrl.set(livePageUrl);
            pageInfo.set(meta);
            pageDisplay.set(display);
            highlightBoxes.set(boxes);
            pageFailure.set("");
        } catch (err) {
            if (objectUrl) {
                URL.revokeObjectURL(objectUrl);
                objectUrl = null;
            }
            if (!isCurrent(request)) return;
            pageFailure.set(errorMessage(err, `第 ${current.sectionIndex + 1} 页的页图不可用。`));
            pageDisplay.set(props.compact ? DEFAULT_COMPACT_DISPLAY : DEFAULT_DISPLAY);
            highlightBoxes.set([]);
            highlightMessage.set(
                `第 ${current.sectionIndex + 1} 页：页图信息无效，无法显示页图或精确高亮。`,
            );
        }
    };

    const loadSurfaceData = async (): Promise<void> => {
        const request = ++requestSeq;
        releasePageUrl();
        pageInfo.set(null);
        pageDisplay.set(null);
        highlightBoxes.set([]);
        highlightMessage.set("");
        pageFailure.set("");
        loading.set(true);
        error.set("");
        statusMsg.set("");

        if (!parsed) {
            error.set(`无效的受控阅读面句柄：${props.surfaceId}`);
            loading.set(false);
            return;
        }
        try {
            const text = loadSectionText(parsed);
            if (!isCurrent(request)) return;
            sectionText.set(text);

            const saved = callSlotKnowledge(props.mountId, {
                kind: "saved_selections",
                format: parsed.format,
                document_id: parsed.documentId,
                section_index: parsed.sectionIndex,
            });
            if (!isCurrent(request)) return;
            savedRanges.set(sanitizeRanges(saved.ranges));

            if (parsed.format !== "pdf") {
                highlightMessage.set("EPUB 暂不支持原生几何高亮。");
                pageDisplay.set(props.compact ? DEFAULT_COMPACT_DISPLAY : DEFAULT_DISPLAY);
            } else {
                await loadPage(parsed, request);
            }
        } catch (err) {
            if (isCurrent(request)) {
                error.set(errorMessage(err, "加载阅读面失败"));
                highlightBoxes.set([]);
                highlightMessage.set("");
            }
        } finally {
            if (isCurrent(request)) loading.set(false);
        }
    };

    const handleCapture = () => {
        if (!parsed || saving()) return;
        if (parsed.mode === "focus") {
            statusMsg.set("聚焦摘录视图不允许新增摘录。");
            return;
        }
        const text = sectionText();
        const range = resolveQuoteRange(text, quoteDraft());
        if (!range.ok) {
            statusMsg.set(`定位失败：${range.error}`);
            return;
        }
        const quote = Array.from(text).slice(range.start, range.end).join("");
        if (!quote.trim()) {
            statusMsg.set("定位失败：当前章节无可摘录文本。");
            return;
        }

        const operationId = `surface-cap-${Date.now()}-${nextSurfaceReq++}`;
        saving.set(true);
        statusMsg.set("");
        try {
            if (parsed.format === "epub") {
                callSlotKnowledge(props.mountId, {
                    kind: "capture_epub_selection",
                    operation_id: operationId,
                    document_id: parsed.documentId,
                    chapter_index: parsed.sectionIndex,
                    start: range.start,
                    end: range.end,
                    quote,
                    note: noteDraft().trim(),
                });
            } else {
                const result = JSON.parse(
                    LibraryHost.captureSelection(
                        parsed.documentId,
                        parsed.sectionIndex,
                        range.start,
                        range.end,
                        noteDraft().trim(),
                        operationId,
                    ),
                );
                if (!result?.ok) {
                    throw new Error(String(result?.error ?? "PDF 选段摘录失败"));
                }
            }
            quoteDraft.set("");
            noteDraft.set("");
            statusMsg.set("已保存选段摘录与个人想法。");
            void loadSurfaceData();
            props.onCaptured?.();
        } catch (err) {
            statusMsg.set(`摘录失败：${errorMessage(err, "保存失败")}`);
        } finally {
            saving.set(false);
        }
    };

    effect(() => {
        void loadSurfaceData();
        return () => {
            requestSeq += 1;
            releasePageUrl();
        };
    });

    const display = () =>
        pageDisplay() ?? (props.compact ? DEFAULT_COMPACT_DISPLAY : DEFAULT_DISPLAY);
    const pageAccessibilityLabel = parsed
        ? `${parsed.format === "pdf" ? "PDF" : "EPUB"} 第 ${parsed.sectionIndex + 1} ${parsed.format === "pdf" ? "页" : "章"}受控阅读面`
        : "受控阅读面";

    return (
        <View
            accessibilityLabel={pageAccessibilityLabel}
            style={{
                padding: props.compact ? 12 : 16,
                borderRadius: 10,
                borderWidth: 1,
                borderColor: ink("border-subtle"),
                backgroundColor: ink("surface-card"),
                gap: 12,
            }}
        >
            <View
                style={{
                    flexDirection: "row",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 8,
                }}
            >
                <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
                    <Badge icon="book" size={24} tone="surface-accent" />
                    <Copy strong>
                        {parsed ? formatLabel(parsed.format, parsed.sectionIndex) : "受控阅读视口"}
                    </Copy>
                </View>
                <Copy size={11} muted>
                    {() => `已保存高亮 ${savedRanges().length} 处`}
                </Copy>
            </View>
            <Rule />
            {() => {
                if (loading()) {
                    return (
                        <Copy size={12} muted>
                            正在通过宿主受控通道加载章节内容…
                        </Copy>
                    );
                }
                if (error()) {
                    return (
                        <View style={{ gap: 8 }}>
                            <Copy size={12}>{error()}</Copy>
                            <View style={{ flexDirection: "row" }}>
                                <Tool
                                    label="重新加载受控阅读面"
                                    text="重试"
                                    onPress={() => void loadSurfaceData()}
                                />
                            </View>
                        </View>
                    );
                }
                const dimensions = display();
                return (
                    <View style={{ gap: 12 }}>
                        {parsed?.format === "pdf" || pageUrl() ? (
                            <View
                                style={{
                                    alignItems: "center",
                                    padding: 8,
                                    borderRadius: 8,
                                    backgroundColor: ink("surface-inset"),
                                    gap: 6,
                                }}
                            >
                                <View
                                    style={{
                                        position: "relative",
                                        width: dimensions.width,
                                        height: dimensions.height,
                                        overflow: "hidden",
                                    }}
                                >
                                    {pageUrl() ? (
                                        <Image
                                            src={pageUrl()}
                                            style={{ width: dimensions.width, height: dimensions.height }}
                                        />
                                    ) : null}
                                    {highlightBoxes().length > 0 ? (
                                        <Canvas
                                            style={{
                                                position: "absolute",
                                                left: 0,
                                                top: 0,
                                                width: dimensions.width,
                                                height: dimensions.height,
                                                pointerEvents: "none",
                                            }}
                                            draw={(cx) => {
                                                cx.fillStyle = "rgba(250, 204, 21, 0.32)";
                                                for (const [x, y, width, height] of highlightBoxes()) {
                                                    cx.fillRect(x, y, width, height);
                                                }
                                            }}
                                        />
                                    ) : null}
                                </View>
                                {pageFailure() ? (
                                    <Copy size={11} muted>
                                        {pageFailure()}
                                    </Copy>
                                ) : null}
                                {highlightMessage() ? (
                                    <Copy size={11} muted>
                                        {highlightMessage()}
                                    </Copy>
                                ) : null}
                            </View>
                        ) : pageFailure() ? (
                            <Copy size={12} muted>
                                {pageFailure()}
                            </Copy>
                        ) : null}
                        <Scroll style={{ maxHeight: 280 }}>
                            <Text
                                style={{
                                    fontSize: 14,
                                    lineHeight: 22,
                                    color: ink("text-primary"),
                                    userSelect: "text",
                                }}
                            >
                                {sectionText() || "（本节无纯文本内容）"}
                            </Text>
                        </Scroll>
                        {parsed?.format === "epub" && parsed.mode === "section" && highlightMessage() ? (
                            <Copy size={12} muted>
                                {highlightMessage()}
                            </Copy>
                        ) : null}
                        {parsed?.mode === "focus" ? (
                            <View style={{ gap: 6 }}>
                                <Rule />
                                <Copy size={12} strong>
                                    聚焦摘录 #{parsed.captureId}
                                </Copy>
                                {highlightMessage() && !pageFailure() ? (
                                    <Copy size={12} muted>
                                        {highlightMessage()}
                                    </Copy>
                                ) : null}
                            </View>
                        ) : (
                            <View style={{ gap: 8 }}>
                                <Rule />
                                <Copy size={12} strong>
                                    选段摘录至知识库（受控能力：core.capture.propose）
                                </Copy>
                                <Input
                                    value={quoteDraft()}
                                    placeholder="输入要摘录的原文片段（留空则摘录本节开头）…"
                                    onChange={(event) => quoteDraft.set(event.value)}
                                />
                                <Input
                                    value={noteDraft()}
                                    placeholder="补充你的个人想法（可选）…"
                                    onChange={(event) => noteDraft.set(event.value)}
                                />
                                <View style={{ flexDirection: "row", alignItems: "center", gap: 10 }}>
                                    <Tool
                                        label="保存选段摘录"
                                        text={saving() ? "正在保存…" : "保存摘录"}
                                        disabled={saving()}
                                        onPress={handleCapture}
                                    />
                                    {statusMsg() ? (
                                        <Copy size={12} muted>
                                            {statusMsg()}
                                        </Copy>
                                    ) : null}
                                </View>
                            </View>
                        )}
                    </View>
                );
            }}
        </View>
    );
}
