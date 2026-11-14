import { effect, signal, View, type JSX } from "nsh";
import { PluginSlotHost } from "qusor/pdf-surface";
import { Copy, ink, Tool } from "../ui";
import { DeclarativeUiRenderer } from "./DeclarativeUiRenderer";
import { type UiEventEnvelope, type UiTree } from "./ui_types";

export interface PluginSlotViewProps {
    contributionId: string;
    title: string;
    compact: boolean;
}

type HostAnswer<T> = {
    ok: boolean;
    value?: T;
    error?: string;
};

type MountSlotValue = {
    mount_id: string;
    generation: number;
    contribution_id: string;
    title: string;
    tree: UiTree;
};

type DispatchSlotValue = {
    mount_id: string;
    generation: number;
    tree: UiTree;
};

function parseHostAnswer<T>(raw: string): T {
    const parsed = JSON.parse(raw) as HostAnswer<T>;
    if (!parsed || parsed.ok !== true || !parsed.value) {
        throw new Error(String(parsed?.error ?? "扩展槽位宿主未返回有效响应"));
    }
    return parsed.value;
}

function errorMessage(err: unknown): string {
    if (err instanceof Error && err.message) return err.message;
    return String(err || "扩展槽位执行失败");
}

/**
 * Generic NSH host view for any `ui_slot` plugin contribution.
 *
 * Mounts the contribution through `PluginSlotHost`, renders the validated
 * `UiTree` with `DeclarativeUiRenderer`, and forwards generation-fenced
 * `UiEventEnvelope` events without any package-specific logic.
 */
export function PluginSlotView(props: PluginSlotViewProps): JSX.Element {
    const loading = signal(true);
    const busy = signal(false);
    const error = signal("");
    const mountId = signal("");
    const generation = signal(0);
    const tree = signal<UiTree | null>(null);
    let disposed = false;

    const releaseMount = (id: string) => {
        if (!id) return;
        try {
            void Promise.resolve(PluginSlotHost.unmountSlot(id));
        } catch (_) {
            // Best-effort cleanup when slot view unmounts.
        }
    };

    const mountSlot = async () => {
        const previousMount = mountId();
        loading.set(true);
        error.set("");
        if (previousMount) {
            mountId.set("");
            releaseMount(previousMount);
        }
        try {
            const raw = await Promise.resolve(
                PluginSlotHost.mountSlot(props.contributionId),
            );
            const mounted = parseHostAnswer<MountSlotValue>(raw);
            if (disposed) {
                releaseMount(mounted.mount_id);
                return;
            }
            mountId.set(mounted.mount_id);
            generation.set(mounted.generation);
            tree.set(mounted.tree);
        } catch (err) {
            if (!disposed) {
                error.set(errorMessage(err));
            }
        } finally {
            if (!disposed) {
                loading.set(false);
            }
        }
    };

    const handleEvent = (envelope: UiEventEnvelope) => {
        const activeMount = mountId();
        if (!activeMount || busy()) return;
        busy.set(true);
        error.set("");
        void (async () => {
            try {
                const raw = await Promise.resolve(
                    PluginSlotHost.dispatchEvent(
                        activeMount,
                        JSON.stringify(envelope),
                    ),
                );
                const updated = parseHostAnswer<DispatchSlotValue>(raw);
                if (disposed) return;
                mountId.set(updated.mount_id);
                generation.set(updated.generation);
                tree.set(updated.tree);
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

    return (
        <View
            accessibilityLabel={`${props.title}插件槽位`}
            style={{
                flex: 1,
                minWidth: 0,
                backgroundColor: ink("surface-base"),
            }}
        >
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
                        />
                    </View>
                );
            }}
        </View>
    );
}
