import { openUrl, signal, View } from "nsh";
import { checkForUpdate, downloadUpdate, installAndRestart } from "nsh/updater";
import type { AppUpdate } from "nsh/updater";
import { Button, Dialog } from "nsh/base";
import { AppInfo, NotesHost, VocabHost } from "qusor/pdf-surface";
import { Copy, Glyph, ink, Rule } from "./ui";

/**
 * Software updates: what this copy is, whether the run's update source offers a
 * newer one, the download's progress, and the restart into it. The update source
 * is the app's own `nsh.json` (`updater.github`); no credentials or tokens pass
 * through here.
 */

export const updateOpen = signal(false);
export const appVersion = signal("dev");
export const appBuild = signal<number | null>(null);
export type UpdateState =
    | "idle"
    | "checking"
    | "current"
    | "available"
    | "downloading"
    | "installing"
    | "error";
export const updateState = signal<UpdateState>("idle");
export const updateMessage = signal("");
export const updateFound = signal<AppUpdate | null>(null);
export const updateProgress = signal(0);

const delay = (ms: number) =>
    new Promise((resolve) => setTimeout(resolve, ms));

export async function loadAppInfo(): Promise<any | null> {
    try {
        const raw = JSON.parse(await AppInfo.info());
        if (!raw.ok) return null;
        appVersion.set(String(raw.value.version ?? "dev"));
        appBuild.set(
            typeof raw.value.build === "number" ? raw.value.build : null,
        );
        return raw.value;
    } catch (_) {
        return null;
    }
}

export function openUpdatePanel() {
    updateOpen.set(true);
    void loadAppInfo();
}

export async function checkForUpdates(): Promise<AppUpdate | null> {
    updateState.set("checking");
    updateMessage.set("正在检查更新…");
    updateFound.set(null);
    try {
        const found = await checkForUpdate();
        updateFound.set(found ?? null);
        if (!found) {
            updateState.set("current");
            updateMessage.set("已是最新版本。");
            return null;
        }
        updateState.set("available");
        updateMessage.set(`发现新版本 ${found.version}。`);
        return found;
    } catch (error) {
        updateState.set("error");
        updateMessage.set(`检查更新失败：${(error as Error).message}`);
        return null;
    }
}

/**
 * The one install path both the dialog button and the self-test take: download
 * and verify, then replace the app and restart into it. `installAndRestart`
 * settles only when the update failed, so the success path ends this process.
 */
export async function installUpdate(hooks?: {
    afterDownload?: () => Promise<void>;
}): Promise<boolean> {
    const found = updateFound();
    if (!found) return false;
    if (!found.canInstall) {
        await openUrl(found.url);
        updateMessage.set("当前安装位置不能自行更新；已打开下载页。");
        return false;
    }
    updateState.set("downloading");
    updateProgress.set(0);
    updateMessage.set("正在下载更新…");
    try {
        await downloadUpdate(({ received, total }) => {
            updateProgress.set(total ? received / total : 0);
        });
        updateProgress.set(1);
        updateState.set("installing");
        updateMessage.set("下载完成，正在安装并重启…");
        if (hooks?.afterDownload) await hooks.afterDownload();
        await installAndRestart();
        return true;
    } catch (error) {
        updateState.set("error");
        updateMessage.set(`安装更新失败：${(error as Error).message}`);
        return false;
    }
}

function ProgressBar() {
    // Components run once: a signal read in the body would freeze. Every binding
    // below reads the signal where it is used, which is what NSH follows.
    return (
        <If when={updateState() === "downloading" || updateState() === "installing"}>
            <View
                style={{
                    height: 4,
                    borderRadius: 2,
                    backgroundColor: ink("surface-inset"),
                    overflow: "hidden",
                }}
            >
                <View
                    style={{
                        height: 4,
                        width: `${Math.round(updateProgress() * 100)}%`,
                        backgroundColor: ink("primary"),
                    }}
                />
            </View>
        </If>
    );
}

function ActionButton(props: {
    label: string;
    disabled?: boolean;
    onPress: () => void;
}) {
    return (
        <Button
            accessibilityLabel={props.label}
            disabled={props.disabled ?? false}
            onPress={props.onPress}
            style={{
                height: 30,
                paddingHorizontal: 13,
                borderRadius: 7,
                borderWidth: 1,
                borderColor: ink("border-control"),
                backgroundColor: ink("surface-control"),
                flexDirection: "row",
                alignItems: "center",
            }}
        >
            <Copy size={12} strong>
                {props.label}
            </Copy>
        </Button>
    );
}

export function UpdateDialog(props: { frameWidth: number }) {
    // Read signals in the bindings that use them (`updateState()` inside `when`),
    // never into a local: the component body runs once, so a local freezes.
    return (
        <Dialog
            open={updateOpen()}
            onOpenChange={({ open }) => updateOpen.set(open)}
            animation="none"
            backdropStyle={{ backgroundColor: ink("surface-scrim") }}
            style={{
                position: "absolute",
                top: 120,
                left: Math.max(18, (props.frameWidth - 470) / 2),
                width: 470,
                padding: 22,
                gap: 14,
                borderRadius: 14,
                borderCurve: "continuous",
                backgroundColor: ink("surface-floating"),
                borderWidth: 1,
                borderColor: ink("border-control"),
                boxShadow: "0 16px 60px rgba(0,0,0,0.22)",
            }}
        >
            <View
                style={{
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                }}
            >
                <Glyph name="arrow" size={16} />
                <Copy size={14} strong>
                    软件更新
                </Copy>
                <View style={{ flex: 1 }} />
                <Button
                    accessibilityLabel="关闭更新"
                    onPress={() => updateOpen.set(false)}
                    style={{
                        width: 24,
                        height: 24,
                        borderRadius: 6,
                        alignItems: "center",
                        justifyContent: "center",
                    }}
                >
                    <Glyph name="close" size={14} />
                </Button>
            </View>
            <Rule />
            <View style={{ gap: 3 }}>
                <Copy size={11} muted>
                    当前版本
                </Copy>
                <Copy size={13} strong>
                    {`QUSOR ${appVersion()}`}
                </Copy>
            </View>
            <Copy size={12} muted>
                {updateMessage() || "从应用自己的更新源检查新版本。"}
            </Copy>
            <If when={updateState() === "available"}>
                <View style={{ gap: 6 }}>
                    <Copy size={11} muted>
                        新版本 {updateFound()?.version ?? ""}
                    </Copy>
                    <Copy size={12} lines={4}>
                        {String(updateFound()?.notes ?? "此版本没有附说明。")}
                    </Copy>
                </View>
            </If>
            <ProgressBar />
            <View
                style={{
                    flexDirection: "row",
                    gap: 8,
                    alignItems: "center",
                }}
            >
                <If when={updateState() === "available"}>
                    <ActionButton
                        label={
                            updateFound()?.canInstall
                                ? "下载并安装"
                                : "打开下载页"
                        }
                        onPress={() => {
                            const found = updateFound();
                            if (found?.canInstall) void installUpdate();
                            else if (found)
                                void openUrl(found.url).catch(() => {});
                        }}
                    />
                </If>
                <If
                    when={
                        updateState() !== "available" &&
                        updateState() !== "downloading" &&
                        updateState() !== "installing"
                    }
                >
                    <ActionButton
                        label={
                            updateState() === "checking"
                                ? "检查中…"
                                : "检查更新"
                        }
                        disabled={updateState() === "checking"}
                        onPress={() => {
                            void checkForUpdates();
                        }}
                    />
                </If>
                <If
                    when={
                        updateState() === "downloading" ||
                        updateState() === "installing"
                    }
                >
                    <ActionButton
                        label={
                            updateState() === "installing"
                                ? "正在安装…"
                                : "正在下载…"
                        }
                        disabled
                        onPress={() => {}}
                    />
                </If>
                <View style={{ flex: 1 }} />
                <Copy size={11} muted>
                    更新源 csic21/qusor · GitHub Releases
                </Copy>
            </View>
        </Dialog>
    );
}

async function receipt(step: string, payload: unknown) {
    try {
        await AppInfo.receipt(step, JSON.stringify(payload));
    } catch (_) {
        // Receipts are evidence only: their absence must not change the flow.
    }
}

async function readHosts() {
    let vocab = false;
    let notes = false;
    try {
        const raw = JSON.parse(await VocabHost.decks());
        vocab = raw.ok === true;
    } catch (_) {
        vocab = false;
    }
    try {
        const raw = JSON.parse(await NotesHost.page());
        notes = raw.ok === true;
    } catch (_) {
        notes = false;
    }
    return { vocab, notes };
}

/**
 * The self-test entry: `QUSOR_UPDATE_EVIDENCE` (or the marker an earlier evidence
 * build armed) starts it. It takes the same check and install paths as the
 * dialog, pauses so the harness can capture each window state, and leaves a
 * receipt per step. The restarted build reports its own version and readable
 * Core, then quits.
 */
export async function runUpdateEvidence() {
    const info = await loadAppInfo();
    if (!info?.evidence?.active) return;
    const dir = String(info.evidence.dir ?? "");
    if (!dir) return;
    if (info.evidence.restarted) {
        await delay(1600);
        const readable = await readHosts();
        await receipt("restarted", {
            version: info.version,
            build: info.build,
            id: info.id,
            data_dir: info.data_dir,
            restarted: true,
            vocab_readable: readable.vocab,
            notes_readable: readable.notes,
        });
        await delay(3200);
        try {
            await AppInfo.consume();
        } catch (_) {}
        try {
            await AppInfo.finish();
        } catch (_) {}
        return;
    }
    openUpdatePanel();
    await delay(1400);
    await receipt("started", {
        version: info.version,
        build: info.build,
        id: info.id,
        data_dir: info.data_dir,
    });
    const found = await checkForUpdates();
    await receipt("checked", {
        state: updateState(),
        of_version: info.version,
        found: found
            ? {
                  version: found.version,
                  can_install: found.canInstall,
                  notes: found.notes ?? null,
                  url: found.url,
              }
            : null,
    });
    await delay(5000);
    if (!found?.canInstall) {
        await delay(600);
        try {
            await AppInfo.finish();
        } catch (_) {}
        return;
    }
    try {
        await AppInfo.arm();
    } catch (error) {
        await receipt("arm-failed", { error: String(error) });
        return;
    }
    await receipt("armed", { target: found.version });
    await installUpdate({
        afterDownload: async () => {
            await receipt("downloaded", {
                target: found.version,
                progress: updateProgress(),
            });
            await delay(4000);
        },
    });
    await receipt("install-failed", {
        state: updateState(),
        message: updateMessage(),
    });
    await delay(1200);
    try {
        await AppInfo.finish();
    } catch (_) {}
}
