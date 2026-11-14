import { effect, Image, Scroll, signal, Text, View, type JSX } from "nsh";
import { Input } from "nsh/base";
import { LibraryHost, PluginSlotHost } from "qusor/pdf-surface";
import { Badge, Copy, ink, Rule, Tool } from "../ui";
import { type ControlledSurfaceKind } from "./DeclarativeUiRenderer";

export interface ControlledDocumentSurfaceProps {
    mountId: string;
    surfaceId: string;
    surfaceKind: ControlledSurfaceKind;
    compact?: boolean;
    onCaptured?: () => void;
}

interface ParsedDocumentHandle {
    format: "pdf" | "epub";
    documentId: number;
    sectionIndex: number;
}

interface SavedRangeItem {
    capture_id: number;
    start: number;
    end: number;
}

let nextSurfaceReq = 1;

function parseDocumentHandle(surfaceId: string): ParsedDocumentHandle | null {
    const parts = surfaceId.trim().split(":");
    if (parts.length < 3) return null;
    const formatRaw = parts[parts.length - 3];
    const docRaw = Number(parts[parts.length - 2]);
    const secRaw = Number(parts[parts.length - 1]);
    if (
        (formatRaw !== "pdf" && formatRaw !== "epub") ||
        !Number.isSafeInteger(docRaw) ||
        docRaw <= 0 ||
        !Number.isSafeInteger(secRaw) ||
        secRaw < 0
    ) {
        return null;
    }
    return {
        format: formatRaw,
        documentId: docRaw,
        sectionIndex: secRaw,
    };
}

function callSlotKnowledge(mountId: string, operation: Record<string, unknown>): any {
    const reqId = `surface-req-${nextSurfaceReq++}`;
    const raw = PluginSlotHost.slotKnowledgeCall(
        mountId,
        JSON.stringify({
            protocol_version: 1,
            api_version: { major: 0, minor: 8 },
            request_id: reqId,
            operation,
        }),
    );
    const outer = JSON.parse(raw);
    if (!outer?.ok || !outer.value) {
        throw new Error(String(outer?.error ?? "正交知识协议调用失败"));
    }
    const result = outer.value.result;
    if (!result || result.kind === "error") {
        throw new Error(
            String(result?.error?.message ?? "正交知识操作返回错误"),
        );
    }
    return result;
}

export function ControlledDocumentSurface(
    props: ControlledDocumentSurfaceProps,
): JSX.Element {
    const parsed = parseDocumentHandle(props.surfaceId);
    const loading = signal(true);
    const error = signal("");
    const sectionText = signal("");
    const savedRanges = signal<SavedRangeItem[]>([]);
    const pdfPageFile = signal<File | null>(null);
    const pdfPageInfo = signal("");
    const quoteDraft = signal("");
    const noteDraft = signal("");
    const statusMsg = signal("");
    const saving = signal(false);

    const loadSurfaceData = async () => {
        if (!parsed) {
            loading.set(false);
            error.set(`无效的受控阅读面句柄：${props.surfaceId}`);
            return;
        }
        loading.set(true);
        error.set("");
        statusMsg.set("");
        try {
            let fullText = "";
            let offset = 0;
            for (let chunkIdx = 0; chunkIdx < 16; chunkIdx += 1) {
                const chunkRes = callSlotKnowledge(props.mountId, {
                    kind: "document_chunk",
                    format: parsed.format,
                    document_id: parsed.documentId,
                    section_index: parsed.sectionIndex,
                    offset,
                    max_bytes: 2048,
                });
                fullText += String(chunkRes.text ?? "");
                const nextOffset = chunkRes.next_offset;
                if (
                    typeof nextOffset !== "number" ||
                    !Number.isSafeInteger(nextOffset) ||
                    nextOffset <= offset
                ) {
                    break;
                }
                offset = nextOffset;
            }
            sectionText.set(fullText);

            const savedRes = callSlotKnowledge(props.mountId, {
                kind: "saved_selections",
                format: parsed.format,
                document_id: parsed.documentId,
                section_index: parsed.sectionIndex,
            });
            savedRanges.set(
                Array.isArray(savedRes.ranges) ? savedRes.ranges : [],
            );

            if (
                parsed.format === "pdf" &&
                typeof LibraryHost?.pageImage === "function"
            ) {
                try {
                    const rendered = await LibraryHost.pageImage(
                        parsed.documentId,
                        parsed.sectionIndex,
                    );
                    pdfPageFile.set(rendered?.file ?? null);
                    pdfPageInfo.set(String(rendered?.info ?? ""));
                } catch (_) {
                    pdfPageFile.set(null);
                }
            } else {
                pdfPageFile.set(null);
            }
        } catch (err: any) {
            error.set(String(err?.message ?? err ?? "加载阅读面失败"));
        } finally {
            loading.set(false);
        }
    };

    const handleCapture = () => {
        if (!parsed || saving()) return;
        const text = sectionText();
        const rawQuote = quoteDraft().trim();
        const targetQuote = rawQuote || text.slice(0, Math.min(text.length, 64)).trim();
        if (!targetQuote) {
            statusMsg.set("当前章节无可摘录文本。");
            return;
        }
        const byteIndex = text.indexOf(targetQuote);
        const startScalar =
            byteIndex >= 0
                ? Array.from(text.slice(0, byteIndex)).length
                : 0;
        const endScalar =
            startScalar + Math.max(1, Array.from(targetQuote).length);
        const opId = `surface-cap-${Date.now()}-${nextSurfaceReq++}`;

        saving.set(true);
        statusMsg.set("");
        try {
            if (parsed.format === "epub") {
                callSlotKnowledge(props.mountId, {
                    kind: "capture_epub_selection",
                    operation_id: opId,
                    document_id: parsed.documentId,
                    chapter_index: parsed.sectionIndex,
                    start: startScalar,
                    end: endScalar,
                    quote: targetQuote,
                    note: noteDraft().trim(),
                });
            } else {
                const raw = LibraryHost.captureSelection(
                    parsed.documentId,
                    parsed.sectionIndex,
                    startScalar,
                    endScalar,
                    noteDraft().trim(),
                    opId,
                );
                const res = JSON.parse(raw);
                if (!res?.ok) {
                    throw new Error(String(res?.error ?? "PDF 选段摘录失败"));
                }
            }
            quoteDraft.set("");
            noteDraft.set("");
            statusMsg.set("已保存选段摘录与个人想法。");
            void loadSurfaceData();
            props.onCaptured?.();
        } catch (err: any) {
            statusMsg.set(`摘录失败：${String(err?.message ?? err)}`);
        } finally {
            saving.set(false);
        }
    };

    effect(() => {
        void loadSurfaceData();
    });

    return (
        <View
            accessibilityLabel={`受控原生阅读面 ${props.surfaceId}`}
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
                        {parsed
                            ? `${parsed.format.toUpperCase()} 原生受控视口 · 第 ${parsed.sectionIndex + 1} ${parsed.format === "pdf" ? "页" : "章"}`
                            : "受控阅读视口"}
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
                const pageFile = pdfPageFile();
                return (
                    <View style={{ gap: 12 }}>
                        {pageFile ? (
                            <View
                                style={{
                                    alignItems: "center",
                                    padding: 8,
                                    borderRadius: 8,
                                    backgroundColor: ink("surface-inset"),
                                }}
                            >
                                <Image
                                    source={pageFile}
                                    style={{
                                        width: props.compact ? 280 : 480,
                                        height: props.compact ? 380 : 640,
                                    }}
                                />
                                {pdfPageInfo() ? (
                                    <Copy size={11} muted>
                                        {pdfPageInfo()}
                                    </Copy>
                                ) : null}
                            </View>
                        ) : null}
                        <Scroll style={{ maxHeight: 280 }}>
                            <Text
                                selectable
                                style={{
                                    fontSize: 14,
                                    lineHeight: 22,
                                    color: ink("text-primary"),
                                }}
                            >
                                {sectionText() || "（本节无纯文本内容）"}
                            </Text>
                        </Scroll>
                        <Rule />
                        <View style={{ gap: 8 }}>
                            <Copy size={12} strong>
                                选段摘录至知识库（受控能力：core.capture.propose）
                            </Copy>
                            <Input
                                value={quoteDraft()}
                                placeholder="输入要摘录的原文片段（留空则摘录本节开头）…"
                                onChange={(val: string) => quoteDraft.set(val)}
                            />
                            <Input
                                value={noteDraft()}
                                placeholder="补充你的个人想法（可选）…"
                                onChange={(val: string) => noteDraft.set(val)}
                            />
                            <View
                                style={{
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 10,
                                }}
                            >
                                <Tool
                                    label="保存选段摘录"
                                    text={saving() ? "正在保存…" : "保存摘录"}
                                    active
                                    onPress={handleCapture}
                                />
                                {statusMsg() ? (
                                    <Copy size={12} muted>
                                        {statusMsg()}
                                    </Copy>
                                ) : null}
                            </View>
                        </View>
                    </View>
                );
            }}
        </View>
    );
}
