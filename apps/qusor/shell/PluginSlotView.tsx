import { effect, Picker, signal, View, type JSX } from "nsh";
import { LibraryHost, PluginSlotHost } from "qusor/pdf-surface";
import { Copy, ink, Tool } from "../ui";
import { ControlledDocumentSurface } from "./ControlledDocumentSurface";
import { DeclarativeUiRenderer } from "./DeclarativeUiRenderer";
import {
    PLUGIN_UI_PROTOCOL,
    type UiEventEnvelope,
    type UiTree,
} from "./ui_types";
import {
    parseOpenSourceAction,
    type PluginNavigation,
    type PluginOpenResult,
} from "./plugin_navigation";

const MAX_IMPORT_BYTES = 16 * 1024 * 1024;

export interface PluginSlotViewProps {
    contributionId: string;
    title: string;
    compact?: boolean;
    navigation: PluginNavigation | null;
    onOpenSource: (navigation: PluginNavigation) => PluginOpenResult;
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
    let mountRequest = 0;
    let eventRequest = 0;
    let importRequest = 0;

    const releaseMount = (id: string) => {
        if (!id) return;
        try {
            void PluginSlotHost.unmountSlot(id);
        } catch (_) {
            // Best-effort cleanup when unmounting or remounting.
        }
    };

    const mountSlot = async (
        contributionId: string,
        navigation: PluginNavigation | null,
        compact: boolean,
    ) => {
        const request = ++mountRequest;
        eventRequest += 1;
        importRequest += 1;
        const previousMount = mountId();
        if (previousMount) {
            mountId.set("");
            releaseMount(previousMount);
        }
        mountId.set("");
        generation.set(0);
        tree.set(null);
        busy.set(false);
        importStatus.set("");
        loading.set(true);
        error.set("");
        try {
            const hostAny = PluginSlotHost as any;
            const raw = navigation
                ? typeof hostAny.mountSlotCompactWithNavigation === "function"
                    ? await Promise.resolve(
                          hostAny.mountSlotCompactWithNavigation(
                              contributionId,
                              compact,
                              JSON.stringify(navigation),
                          ),
                      )
                    : (() => {
                          throw new Error(
                              "当前宿主不支持带导航的插件槽位装载。",
                          );
                      })()
                : typeof hostAny.mountSlotCompact === "function"
                  ? await Promise.resolve(
                        hostAny.mountSlotCompact(
                            contributionId,
                            compact,
                        ),
                    )
                  : await Promise.resolve(
                        PluginSlotHost.mountSlot(contributionId),
                    );
            const response = parseEnvelope<MountSlotPayload>(raw);
            if (!response.ok || !response.value) {
                if (request === mountRequest) {
                    error.set(response.error ?? "无法装载插件槽位。");
                    tree.set(null);
                }
                return;
            }
            if (request !== mountRequest) {
                releaseMount(response.value.mount_id);
                return;
            }
            mountId.set(response.value.mount_id);
            generation.set(response.value.generation);
            tree.set(response.value.tree);
        } catch (err) {
            if (request === mountRequest) {
                error.set(errorMessage(err));
                tree.set(null);
            }
        } finally {
            if (request === mountRequest) {
                loading.set(false);
            }
        }
    };

    const handleEvent = (envelope: UiEventEnvelope) => {
        const currentMount = mountId();
        if (!currentMount || busy()) return;
        const request = ++eventRequest;
        busy.set(true);
        error.set("");
        if (
            Array.isArray(envelope.action_id) ||
            typeof envelope.action_id !== "string"
        ) {
            error.set("插件事件动作无效，已阻止处理。");
            busy.set(false);
            return;
        }
        const actionId = envelope.action_id;
        if (actionId.startsWith("open_source:")) {
            const parsed = parseOpenSourceAction(actionId);
            if (!parsed.ok) {
                error.set(parsed.error);
                busy.set(false);
                return;
            }
            try {
                const result = props.onOpenSource(parsed.navigation);
                if (result && !result.ok) {
                    error.set(result.error);
                }
            } catch (err) {
                error.set(errorMessage(err));
            } finally {
                busy.set(false);
            }
            return;
        }
        if (
            actionId === "open_source" ||
            actionId.startsWith("open_source")
        ) {
            error.set("插件请求了无效的原文跳转动作，已阻止跳转。");
            busy.set(false);
            return;
        }
        void (async () => {
            try {
                const raw = await Promise.resolve(
                    PluginSlotHost.dispatchEvent(
                        currentMount,
                        JSON.stringify(envelope),
                    ),
                );
                if (request !== eventRequest) return;
                const response = parseEnvelope<DispatchEventPayload>(raw);
                if (!response.ok || !response.value) {
                    error.set(response.error ?? "插件事件分发失败。");
                    return;
                }
                generation.set(response.value.generation);
                tree.set(response.value.tree);
            } catch (err) {
                if (request === eventRequest) {
                    error.set(errorMessage(err));
                }
            } finally {
                if (request === eventRequest) {
                    busy.set(false);
                }
            }
        })();
    };

    const importNativeDocument = async (format: "pdf" | "epub") => {
        if (busy()) return;
        const request = ++importRequest;
        busy.set(true);
        importStatus.set("");
        let refreshEnvelope: UiEventEnvelope | null = null;
        try {
            const picked = await Picker.pickFiles({
                multiple: false,
                filters: [
                    format === "pdf"
                        ? { name: "PDF 文档", extensions: ["pdf"] }
                        : { name: "EPUB 书籍", extensions: ["epub"] },
                ],
            });
            if (request !== importRequest) return;
            if (picked.length === 0) return;
            const file = picked[0].file;
            if (file.size > MAX_IMPORT_BYTES) {
                importStatus.set(
                    `${format.toUpperCase()} 文件不能超过 16 MiB。`,
                );
                return;
            }
            const bytes = new Uint8Array(await file.arrayBuffer());
            if (request !== importRequest) return;
            const raw =
                format === "pdf"
                    ? await LibraryHost.importPdf(file.name, bytes)
                    : await LibraryHost.importEpub(file.name, bytes);
            if (request !== importRequest) return;
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
                refreshEnvelope = {
                    protocol_version: PLUGIN_UI_PROTOCOL,
                    mount_id: currentMount,
                    generation: generation(),
                    node_id: "library_refresh_btn",
                    action_id: "refresh_library",
                    action: { kind: "press" },
                };
            }
        } catch (err) {
            if (request === importRequest) {
                importStatus.set(`导入失败：${errorMessage(err)}`);
            }
        } finally {
            if (request === importRequest) {
                busy.set(false);
                if (refreshEnvelope) handleEvent(refreshEnvelope);
            }
        }
    };

    effect(() => {
        const contributionId = props.contributionId;
        const navigation = props.navigation;
        const compact = Boolean(props.compact);
        void mountSlot(contributionId, navigation, compact);
        return () => {
            mountRequest += 1;
            eventRequest += 1;
            importRequest += 1;
            const currentMount = mountId();
            mountId.set("");
            tree.set(null);
            generation.set(0);
            busy.set(false);
            if (currentMount) {
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
                                    onPress={() =>
                                        void mountSlot(
                                            props.contributionId,
                                            props.navigation,
                                            Boolean(props.compact),
                                        )
                                    }
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
                                    onPress={() =>
                                        void mountSlot(
                                            props.contributionId,
                                            props.navigation,
                                            Boolean(props.compact),
                                        )
                                    }
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
