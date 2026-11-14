import { effect, Picker, signal, View, type JSX } from "nsh";
import { LibraryHost, PluginSlotHost } from "qusor/pdf-surface";
import { Copy, ink, Tool } from "../ui";
import { ControlledDocumentSurface } from "./ControlledDocumentSurface";
import {
    DeclarativeUiRenderer,
    PLUGIN_UI_PROTOCOL,
    type UiEventEnvelope,
    type UiTree,
} from "./DeclarativeUiRenderer";

const MAX_IMPORT_BYTES = 16 * 1024 * 1024;

export interface PluginSlotViewProps {
    contributionId: string;
    title: string;
    compact?: boolean;
}

interface MountSlotPayload {
    mount_id: string;
    generation: number;
    contribution_id: string;
    title: string;
    tree: UiTree;
}

interface DispatchEventPayload {
    mount_id: string;
    generation: number;
    tree: UiTree;
}

interface HostEnvelope<T> {
    ok: boolean;
    value?: T;
    error?: string;
}

function parseEnvelope<T>(raw: string): HostEnvelope<T> {
    try {
        const parsed = JSON.parse(raw);
        if (parsed && typeof parsed === "object" && typeof parsed.ok === "boolean") {
            return parsed as HostEnvelope<T>;
        }
    } catch (_) {
        // Fall through to structured error.
    }
    return {
        ok: false,
        error: "插件宿主返回了无法解析的响应。",
    };
}

function errorMessage(err: unknown): string {
    if (err instanceof Error && err.message) {
        return err.message;
    }
    if (typeof err === "string" && err.trim().length > 0) {
        return err;
    }
    return "插件槽位调用失败。";
}

export function PluginSlotView(props: PluginSlotViewProps): JSX.Element {
    const mountId = signal<string>("");
    const generation = signal<number>(0);
    const tree = signal<UiTree | null>(null);
    const loading = signal<boolean>(true);
    const busy = signal<boolean>(false);
    const error = signal<string>("");
    const importStatus = signal<string>("");
    let disposed = false;

    const releaseMount = (id: string) => {
        if (!id) return;
        try {
            void PluginSlotHost.unmountSlot(id);
        } catch (_) {
            // Best-effort cleanup when unmounting or remounting.
        }
    };

    const mountSlot = async () => {
        const previousMount = mountId();
        if (previousMount) {
            mountId.set("");
            releaseMount(previousMount);
        }
        loading.set(true);
        error.set("");
        try {
            const hostAny = PluginSlotHost as any;
            const raw =
                typeof hostAny.mountSlotCompact === "function"
                    ? await Promise.resolve(
                          hostAny.mountSlotCompact(
                              props.contributionId,
                              Boolean(props.compact),
                          ),
                      )
                    : await Promise.resolve(
                          PluginSlotHost.mountSlot(props.contributionId),
                      );
            if (disposed) return;
            const response = parseEnvelope<MountSlotPayload>(raw);
            if (!response.ok || !response.value) {
                error.set(response.error ?? "无法装载插件槽位。");
                tree.set(null);
                return;
            }
            mountId.set(response.value.mount_id);
            generation.set(response.value.generation);
            tree.set(response.value.tree);
        } catch (err) {
            if (!disposed) {
                error.set(errorMessage(err));
                tree.set(null);
            }
        } finally {
            if (!disposed) {
                loading.set(false);
            }
        }
    };

    const handleEvent = (envelope: UiEventEnvelope) => {
        const currentMount = mountId();
        if (!currentMount || busy()) return;
        busy.set(true);
        error.set("");
        void (async () => {
            try {
                const raw = await Promise.resolve(
                    PluginSlotHost.dispatchEvent(
                        currentMount,
                        JSON.stringify(envelope),
                    ),
                );
                if (disposed) return;
                const response = parseEnvelope<DispatchEventPayload>(raw);
                if (!response.ok || !response.value) {
                    error.set(response.error ?? "插件事件分发失败。");
                    return;
                }
                generation.set(response.value.generation);
                tree.set(response.value.tree);
            } catch (err) {
                if (!disposed) {
                    error.set(errorMessage(err));
                }
            } finally {
                if (!disposed) {
                    busy.set(false);
                }
            }
        })();
    };

    const importNativeDocument = async (format: "pdf" | "epub") => {
        if (busy()) return;
        importStatus.set("");
        try {
            const picked = await Picker.pickFiles({
                multiple: false,
                filters: [
                    format === "pdf"
                        ? { name: "PDF 文档", extensions: ["pdf"] }
                        : { name: "EPUB 书籍", extensions: ["epub"] },
                ],
            });
            if (picked.length === 0) return;
            const file = picked[0].file;
            if (file.size > MAX_IMPORT_BYTES) {
                importStatus.set(
                    `${format.toUpperCase()} 文件不能超过 16 MiB。`,
                );
                return;
            }
            busy.set(true);
            const bytes = new Uint8Array(await file.arrayBuffer());
            const raw =
                format === "pdf"
                    ? await LibraryHost.importPdf(file.name, bytes)
                    : await LibraryHost.importEpub(file.name, bytes);
            const res = JSON.parse(raw);
            if (!res?.ok) {
                importStatus.set(
                    `导入失败：${String(res?.error ?? "未知错误")}`,
                );
                return;
            }
            importStatus.set(`已导入《${file.name}》`);
            const currentMount = mountId();
            if (currentMount) {
                handleEvent({
                    protocol_version: PLUGIN_UI_PROTOCOL,
                    mount_id: currentMount,
                    generation: generation(),
                    node_id: "library_refresh_btn",
                    action_id: "refresh_library",
                    action: { kind: "press" },
                });
            }
        } catch (err) {
            importStatus.set(`导入失败：${errorMessage(err)}`);
        } finally {
            busy.set(false);
        }
    };

    effect(() => {
        void mountSlot();
        return () => {
            disposed = true;
            const currentMount = mountId();
            if (currentMount) {
                mountId.set("");
                releaseMount(currentMount);
            }
        };
    });

    const isLibrarySlot = props.title === "资料库";

    return (
        <View
            accessibilityLabel={`${props.title}插件槽位`}
            style={{
                flex: 1,
                minWidth: 0,
                backgroundColor: ink("surface-base"),
            }}
        >
            {isLibrarySlot ? (
                <View
                    accessibilityLabel="宿主受控文件导入栏"
                    style={{
                        paddingHorizontal: 16,
                        paddingVertical: 10,
                        borderBottomWidth: 1,
                        borderColor: ink("border-subtle"),
                        backgroundColor: ink("surface-toolbar"),
                        flexDirection: "row",
                        alignItems: "center",
                        justifyContent: "space-between",
                        gap: 8,
                    }}
                >
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 8,
                        }}
                    >
                        <Tool
                            label="导入本地 PDF 文件"
                            text="导入 PDF"
                            onPress={() => void importNativeDocument("pdf")}
                        />
                        <Tool
                            label="导入本地 EPUB 书籍"
                            text="导入 EPUB"
                            onPress={() => void importNativeDocument("epub")}
                        />
                    </View>
                    {() =>
                        importStatus() ? (
                            <Copy size={12} muted>
                                {importStatus()}
                            </Copy>
                        ) : null
                    }
                </View>
            ) : null}
            {() => {
                if (loading() && !tree()) {
                    return (
                        <View style={{ padding: 24, gap: 8 }}>
                            <Copy strong>{`正在装载「${props.title}」…`}</Copy>
                            <Copy size={12} muted>
                                正在验证 Wasm 插件签名与能力授权。
                            </Copy>
                        </View>
                    );
                }
                if (error() && !tree()) {
                    return (
                        <View style={{ padding: 24, gap: 12 }}>
                            <View
                                style={{
                                    padding: 16,
                                    borderRadius: 10,
                                    borderWidth: 1,
                                    borderColor: ink("border-danger"),
                                    backgroundColor: ink("surface-danger"),
                                    gap: 6,
                                }}
                            >
                                <Copy strong>{`无法装载「${props.title}」`}</Copy>
                                <Copy size={12} muted>
                                    {error()}
                                </Copy>
                            </View>
                            <View style={{ flexDirection: "row" }}>
                                <Tool
                                    label={`重新装载${props.title}`}
                                    text="重试装载"
                                    onPress={() => void mountSlot()}
                                />
                            </View>
                        </View>
                    );
                }
                const currentTree = tree();
                const currentMount = mountId();
                if (!currentTree || !currentMount) {
                    return (
                        <View style={{ padding: 24 }}>
                            <Copy size={12} muted>
                                扩展槽位尚无界面输出。
                            </Copy>
                        </View>
                    );
                }
                return (
                    <View style={{ flex: 1, minWidth: 0 }}>
                        {error() ? (
                            <View
                                style={{
                                    marginHorizontal: 16,
                                    marginTop: 12,
                                    padding: 12,
                                    borderRadius: 8,
                                    borderWidth: 1,
                                    borderColor: ink("border-danger"),
                                    backgroundColor: ink("surface-danger"),
                                    flexDirection: "row",
                                    alignItems: "center",
                                    justifyContent: "space-between",
                                    gap: 8,
                                }}
                            >
                                <Copy size={12}>{error()}</Copy>
                                <Tool
                                    label="重新同步插件状态"
                                    text="重载"
                                    onPress={() => void mountSlot()}
                                />
                            </View>
                        ) : null}
                        <DeclarativeUiRenderer
                            tree={currentTree}
                            mountId={currentMount}
                            generation={generation()}
                            compact={props.compact}
                            onEvent={handleEvent}
                            renderControlledSurface={(
                                surfaceId,
                                surfaceKind,
                            ) => (
                                <ControlledDocumentSurface
                                    mountId={currentMount}
                                    surfaceId={surfaceId}
                                    surfaceKind={surfaceKind}
                                    compact={props.compact}
                                />
                            )}
                        />
                    </View>
                );
            }}
        </View>
    );
}
