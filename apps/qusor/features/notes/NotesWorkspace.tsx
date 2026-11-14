import { Clipboard, effect, keymap, Scroll, signal, View, type InputRef } from "nsh";
import { Button, Input, Menu, MenuItem, MenuSeparator, TextView } from "nsh/base";
import { Menu as DesktopMenu, type MenuItem as DesktopMenuItem } from "nsh/desktop";
import { BackHandler } from "nsh/mobile";
import { NotesHost } from "qusor/pdf-surface";
import { Badge, Copy, focus, Glyph, ink, menuItem, menuPopup, Rule, TextButton, Tool } from "../../ui";
import { Paragraph, Sheet } from "../../shell/editorial";
import { openLibrarySource, libraryOpenDoc } from "../library/LibraryWorkspace";

const notesQuery = signal(""),
    notesScope = signal("全部想法"),
    notesSelected = signal(""),
    notesNavigation = signal(false),
    notesSelection = signal(""),
    notesCopyStatus = signal(""),
    notesBody = signal<Record<string, string>>({}),
    notesBodyLoading = signal("");
let notesSearchRef: InputRef | null = null;
type Note = {
    id: string;
    title: string;
    document: string;
    page: number;
    quote: string;
    note: string;
    full?: string;
    date: string;
    scope: string;
    starred: boolean;
    missingOriginal: boolean;
    captureId?: number;
    documentId?: number;
    quoteTruncated?: boolean;
    format?: "pdf" | "epub";
};
const notesFixture: Note[] = [
    {
        id: "native-hierarchy",
        title: "原生界面里的层级",
        document: "准原生应用的秩序",
        page: 3,
        quote: "插件拥有自己的工作方式，用户始终拥有回到上层工作区的路径。",
        note: "宿主返回和插件内部返回要写成两个不同的目的地。进入插件后，身份与来源不能一起消失。",
        full: "宿主返回和插件内部返回要写成两个不同的目的地。进入插件后，身份与来源不能一起消失。",
        date: "今天 10:42",
        scope: "当前资料",
        starred: true,
        missingOriginal: false,
    },
    {
        id: "quiet-tools",
        title: "工具应该退到一旁",
        document: "工具退到一旁",
        page: 8,
        quote: "阅读是一种进入内容的方式。写作、讨论和绘图也一样。",
        note: "阅读时不让表单常驻。只有选文后，才把保存和想法动作放到正文附近。",
        full: "阅读时不让表单常驻。只有选文后，才把保存和想法动作放到正文附近。工具应该像书架一样，需要时出现，不需要时退到一旁；但退到一旁不等于消失，用户仍要能从工作区身份和返回路径知道自己在哪。列表用于浏览，详情用于阅读与复制，两者不要同时抢注意力。复制出来的引用必须带上资料标题与页码，否则离开应用之后就无法再核对来源。",
        date: "昨天 21:16",
        scope: "当前资料",
        starred: false,
        missingOriginal: false,
    },
    {
        id: "plugin-boundary",
        title: "知识不属于界面提供者",
        document: "插件与宿主之间",
        page: 5,
        quote: "共同的是工作区，变化的是看待内容的方式。",
        note: "停用插件后，摘录、笔记和来源仍要留在 Core。插件可以被替换，知识身份不能跟着换。",
        full: "停用插件后，摘录、笔记和来源仍要留在 Core。插件可以被替换，知识身份不能跟着换。",
        date: "11 月 7 日",
        scope: "其他资料",
        starred: false,
        missingOriginal: false,
    },
    {
        id: "missing-source",
        title: "原件缺失时的诚实反馈",
        document: "已移走的资料",
        page: 2,
        quote: "来源不可用时就明说，不要把相似文本当成同一条引用。",
        note: "缺少原件、版本不符或权限撤销都应该有可观察的状态。",
        full: "缺少原件、版本不符或权限撤销都应该有可观察的状态。界面不能把“找不到”伪装成“已定位”。",
        date: "11 月 6 日",
        scope: "其他资料",
        starred: false,
        missingOriginal: true,
    },
];
const notesData = signal<Note[]>(notesFixture);
const notesLive = signal(false);
export const notesSource = signal("样例数据");
const notesSourceDetail = signal("窗口内样例");
const notesEditingId = signal("");
const notesEditDraft = signal("");
const notesSaving = signal(false);

function firstLine(value: string) {
    return value.split("\n")[0].trim().slice(0, 60);
}

function noteLocationLabel(note: Note | undefined): string {
    if (!note) return "";
    return note.format === "epub" ? `第 ${note.page} 章` : `第 ${note.page} 页`;
}

async function loadLiveNotes() {
    try {
        const raw = await NotesHost.page();
        const result = JSON.parse(raw);
        if (!result.ok || !Array.isArray(result.value?.notes)) return;
        const page = result.value;
        const activeDocId =
            libraryOpenDoc()?.document_id ?? page.current_document_id;
        const mappedPdf: Note[] = page.notes.map((wire: any): Note => ({
            id: `live-${wire.capture_id}-${wire.commit_rev}`,
            title: firstLine(wire.note_preview) || wire.document_title,
            document: wire.document_title,
            page: wire.page_index + 1,
            quote: wire.quote_preview,
            note: wire.note_preview,
            full: wire.note_truncated ? undefined : wire.note_preview,
            date: `提交 ${wire.commit_rev}`,
            scope:
                wire.document_id === activeDocId ? "当前资料" : "其他资料",
            starred: false,
            missingOriginal: false,
            captureId: wire.capture_id,
            documentId: wire.document_id,
            quoteTruncated: wire.quote_truncated === true,
            format: "pdf",
        }));
        let mappedEpub: Note[] = [];
        const epubHost = (NotesHost as any).epubNotesPage;
        if (typeof epubHost === "function") {
            try {
                const epubRaw = await epubHost.call(NotesHost, "", 50);
                const epubRes = JSON.parse(epubRaw);
                if (epubRes?.ok && Array.isArray(epubRes.value?.notes)) {
                    mappedEpub = epubRes.value.notes.map((wire: any): Note => ({
                        id: `live-epub-${wire.capture_id}-${wire.commit_rev}`,
                        title:
                            firstLine(String(wire.note_preview ?? "")) ||
                            String(wire.document_title ?? "EPUB 书籍"),
                        document: String(wire.document_title ?? "EPUB 书籍"),
                        page:
                            Number(
                                wire.chapter_index ?? wire.page_index ?? 0,
                            ) + 1,
                        quote: String(wire.quote_preview ?? ""),
                        note: String(wire.note_preview ?? ""),
                        full: wire.note_truncated
                            ? undefined
                            : String(wire.note_preview ?? ""),
                        date: `提交 ${wire.commit_rev ?? 0}`,
                        scope:
                            wire.document_id === activeDocId
                                ? "当前资料"
                                : "其他资料",
                        starred: false,
                        missingOriginal: false,
                        captureId: Number(wire.capture_id),
                        documentId: Number(wire.document_id),
                        quoteTruncated: wire.quote_truncated === true,
                        format: "epub",
                    }));
                }
            } catch (_) {
                // Keep PDF notes when EPUB notes endpoint is unavailable.
            }
        }
        notesData.set([...mappedPdf, ...mappedEpub]);
        notesLive.set(true);
        notesSource.set("本机 Core");
        notesSourceDetail.set("由 notes 插件读取");
    } catch (_) {
        // Headless tests and the plain development host keep the fixture, but
        // the window states which source is actually shown.
        notesSource.set("样例数据");
        notesSourceDetail.set("窗口内样例");
    }
}

export function refreshNotes() {
    void loadLiveNotes();
}

/// Evidence-only, driven by App when the host was started with
/// QUSOR_WORKBENCH_EVIDENCE. Captures the rendered window through NotesHost so
/// no screen-recording grant or frontmost window is needed.
export async function captureNotesEvidence() {
    let dir = "";
    try {
        const result = JSON.parse(await NotesHost.evidence());
        if (!result.ok || !result.value?.active) return;
        dir = result.value.dir;
    } catch (_) {
        return;
    }
    if (!dir) return;
    for (let attempt = 0; attempt < 30 && !notesLive(); attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
    await NotesHost.capture(`${dir}/list.png`);
    const first = visibleNotes()[0];
    if (!first) return;
    selectNote(first);
    await new Promise((resolve) => setTimeout(resolve, 500));
    await NotesHost.capture(`${dir}/detail.png`);
    const truncated = visibleNotes().find((note) => noteTruncated(note));
    if (!truncated) return;
    selectNote(truncated);
    await new Promise((resolve) => setTimeout(resolve, 400));
    await NotesHost.capture(`${dir}/truncated.png`);
    loadNoteBody(truncated);
    for (let attempt = 0; attempt < 30 && !notesBody()[truncated.id]; attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
    await NotesHost.capture(`${dir}/full-body.png`);
}

function noteInScope(note: Note, scope: string) {
    return scope === "全部想法" || note.scope === "当前资料";
}

function noteCount(scope: string) {
    return notesData().filter((note) => noteInScope(note, scope)).length;
}

function visibleNotes() {
    return notesData().filter((note) => {
        const query = notesQuery().trim().toLowerCase();
        const matches =
            !query ||
            `${note.title} ${note.document} ${note.quote} ${note.full ?? ""}`
                .toLowerCase()
                .includes(query);
        return noteInScope(note, notesScope()) && matches;
    });
}

function selectedNote() {
    return notesData().find((note) => note.id === notesSelected());
}

function noteTruncated(note: Note) {
    return note.full !== note.note;
}

function noteBodyText(note: Note) {
    return notesBody()[note.id] ?? note.note;
}

function noteCitation(note: Note) {
    return `> ${note.quote}\n\n— 《${note.document}》${noteLocationLabel(note)}`;
}

function startEditingNote(note: Note) {
    notesEditingId.set(note.id);
    notesEditDraft.set(noteBodyText(note));
    notesCopyStatus.set("");
}

async function saveEditedNote(note: Note) {
    if (notesSaving()) return;
    const text = notesEditDraft().trim();
    notesSaving.set(true);
    try {
        const host = NotesHost as any;
        if (
            notesLive() &&
            note.captureId !== undefined &&
            typeof host?.updateNote === "function"
        ) {
            const raw = await host.updateNote(
                note.captureId,
                note.format ?? "pdf",
                text,
            );
            const result = JSON.parse(raw);
            if (!result?.ok) {
                throw new Error(String(result?.error ?? "修改失败"));
            }
            notesBody.set({ ...notesBody(), [note.id]: text });
            notesData.set(
                notesData().map((item) =>
                    item.id === note.id
                        ? {
                              ...item,
                              title: firstLine(text) || item.document,
                              note: text,
                              full: text,
                          }
                        : item,
                ),
            );
            notesEditingId.set("");
            notesCopyStatus.set("已保存修改到本机 Core。");
            void loadLiveNotes();
            return;
        }
        notesBody.set({ ...notesBody(), [note.id]: text });
        notesData.set(
            notesData().map((item) =>
                item.id === note.id
                    ? {
                          ...item,
                          title: firstLine(text) || item.document,
                          note: text,
                          full: text,
                      }
                    : item,
            ),
        );
        notesEditingId.set("");
        notesCopyStatus.set("已更新当前窗口内的想法。");
    } catch (error) {
        notesCopyStatus.set(
            `修改未保存：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    } finally {
        notesSaving.set(false);
    }
}

async function deleteNoteRecord(note: Note) {
    if (notesSaving()) return;
    notesSaving.set(true);
    try {
        const host = NotesHost as any;
        if (
            notesLive() &&
            note.captureId !== undefined &&
            typeof host?.deleteNote === "function"
        ) {
            const raw = await host.deleteNote(
                note.captureId,
                note.format ?? "pdf",
            );
            const result = JSON.parse(raw);
            if (!result?.ok) {
                throw new Error(String(result?.error ?? "删除失败"));
            }
        }
        notesData.set(notesData().filter((item) => item.id !== note.id));
        notesEditingId.set("");
        notesSelected.set("");
        notesSelection.set("");
        notesCopyStatus.set("已删除该条想法记录。");
        if (notesLive()) void loadLiveNotes();
    } catch (error) {
        notesCopyStatus.set(
            `删除未完成：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    } finally {
        notesSaving.set(false);
    }
}

function noteMenuItems(note: Note): DesktopMenuItem[] {
    return [
        {
            label:
                note.captureId === undefined
                    ? "打开所在页（样例）"
                    : "打开所在页",
            disabled: note.missingOriginal,
            onSelect: () => openNoteSource(note),
        },
        { label: "复制引文", onSelect: () => noteCopy(note.quote, "引文") },
        {
            label: "复制想法",
            onSelect: () => noteCopy(noteBodyText(note), "想法"),
        },
        {
            label: "复制引用",
            onSelect: () => noteCopy(noteCitation(note), "引用"),
        },
        { type: "separator" },
        {
            label: noteTruncated(note) ? "读取完整内容" : "内容已完整",
            disabled: !noteTruncated(note),
            onSelect: () => loadNoteBody(note),
        },
        {
            label: "编辑想法",
            onSelect: () => {
                selectNote(note);
                startEditingNote(note);
            },
        },
        {
            label: "删除记录",
            onSelect: () => void deleteNoteRecord(note),
        },
    ];
}

function noteCopy(text: string, label: string) {
    void Clipboard.writeText(text)
        .then(() => notesCopyStatus.set(`已复制${label}。`))
        .catch(() =>
            notesCopyStatus.set("复制未完成，可选中文本手动复制。"),
        );
}

function openNoteSource(note: Note) {
    if (note.missingOriginal) {
        notesCopyStatus.set("原件缺失，无法定位到这一页。");
        return;
    }
    if (note.documentId !== undefined) {
        const opened = openLibrarySource({
            documentId: note.documentId,
            pageIndex: note.page - 1,
            title: note.document,
            quote: note.quote,
            quoteTruncated: note.quoteTruncated === true,
            format: note.format ?? "pdf",
        });
        if (!opened) {
            notesCopyStatus.set(
                "资料库插件当前不可用；引文与想法仍保留在本机 Core。",
            );
        }
        return;
    }
    if (note.scope !== "当前资料") {
        notesCopyStatus.set(
            `先打开《${note.document}》后可定位到${noteLocationLabel(note)}。`,
        );
        return;
    }
    notesCopyStatus.set(
        `已请求打开《${note.document}》${noteLocationLabel(note)}；真实回源由阅读插件承接。`,
    );
}

function loadNoteBody(note: Note) {
    if (notesBody()[note.id] || notesBodyLoading()) return;
    notesBodyLoading.set(note.id);
    if (notesLive() && note.captureId !== undefined) {
        const captureId = note.captureId;
        void (async () => {
            try {
                let offset = 0;
                let text = "";
                const host = NotesHost as any;
                const isEpub = note.format === "epub";
                if (isEpub && typeof host?.epubBody !== "function") {
                    throw Error("EPUB note body unavailable");
                }
                for (let chunk = 0; chunk < 64; chunk += 1) {
                    const raw = isEpub
                        ? await host.epubBody(captureId, "note", offset, 2048)
                        : await NotesHost.body(
                              captureId,
                              "note",
                              offset,
                              2048,
                          );
                    const result = JSON.parse(raw);
                    if (!result.ok || !result.value) throw Error();
                    text += String(result.value.text ?? "");
                    if (
                        result.value.next_offset === null ||
                        result.value.next_offset === undefined
                    )
                        break;
                    const next = Number(result.value.next_offset);
                    if (!Number.isSafeInteger(next) || next <= offset) break;
                    offset = next;
                }
                notesBody.set({ ...notesBody(), [note.id]: text });
            } catch (_) {
                notesCopyStatus.set(
                    "完整内容暂时读取不到；列表预览仍保留，可重试。",
                );
            } finally {
                notesBodyLoading.set("");
            }
        })();
        return;
    }
    setTimeout(() => {
        notesBody.set({ ...notesBody(), [note.id]: note.full ?? "" });
        notesBodyLoading.set("");
    }, 450);
}

function selectNote(note: Note) {
    notesSelected.set(note.id);
    notesSelection.set("");
    notesCopyStatus.set("");
    notesNavigation.set(false);
}

function moveNoteSelection(delta: number) {
    const list = visibleNotes();
    if (list.length === 0) return;
    const current = list.findIndex((note) => note.id === notesSelected());
    const base = current < 0 ? (delta > 0 ? -1 : 0) : current;
    const next = Math.max(0, Math.min(list.length - 1, base + delta));
    selectNote(list[next]);
}

function copyCurrentNote() {
    if (notesSelection()) {
        noteCopy(notesSelection(), "选中内容");
        return;
    }
    const note = selectedNote();
    if (note) noteCopy(noteBodyText(note), "想法");
}

function NotesSidebar(props: { compact: boolean }) {
    return (
        <View
            accessibilityLabel="想法插件导航"
            style={{
                width: props.compact ? "100%" : 214,
                flexShrink: 0,
                borderRightWidth: props.compact ? 0 : 1,
                borderColor: ink("border-subtle"),
                backgroundColor: ink("surface-toolbar"),
            }}
        >
            <View
                style={{
                    height: 48,
                    paddingHorizontal: 14,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                }}
            >
                <Badge icon="pen" size={28} tone="surface-accent" />
                <View style={{ flex: 1, minWidth: 0 }}>
                    <Copy strong>想法</Copy>
                    <Copy size={11} muted>
                        {notesSource() === "本机 Core"
                            ? "本机 Core"
                            : "工作区样例"}
                    </Copy>
                </View>
                <If when={props.compact}>
                    <Tool
                        label="返回想法列表"
                        icon="back"
                        text="想法"
                        size="touch"
                        onPress={() => notesNavigation.set(false)}
                    />
                </If>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View style={{ padding: 10, gap: 15 }}>
                    <View style={{ gap: 3 }}>
                        <Each of={["全部想法", "当前资料"]}>
                            {(scope) => (
                                <Button
                                    accessibilityLabel={`显示${scope()}想法`}
                                    onPress={() => {
                                        notesScope.set(scope());
                                        notesSelected.set("");
                                        notesSelection.set("");
                                        notesCopyStatus.set("");
                                        notesNavigation.set(false);
                                    }}
                                    style={{
                                        height: props.compact ? 44 : 34,
                                        paddingHorizontal: 10,
                                        flexDirection: "row",
                                        alignItems: "center",
                                        gap: 9,
                                        borderRadius: 7,
                                        backgroundColor: ink(
                                            notesScope() === scope()
                                                ? "surface-selected"
                                                : "transparent",
                                        ),
                                    }}
                                    hoverStyle={{
                                        backgroundColor: ink("surface-hover"),
                                    }}
                                    focusStyle={focus()}
                                >
                                    <Glyph
                                        name={
                                            scope() === "当前资料"
                                                ? "document"
                                                : "grid"
                                        }
                                        size={16}
                                        color={ink(
                                            notesScope() === scope()
                                                ? "text-accent"
                                                : "text-secondary",
                                        )}
                                    />
                                    <Copy>{scope()}</Copy>
                                    <View style={{ flex: 1 }} />
                                    <Copy size={11} muted>
                                        {noteCount(scope())}
                                    </Copy>
                                </Button>
                            )}
                        </Each>
                    </View>
                    <View style={{ gap: 4 }}>
                        <View style={{ paddingHorizontal: 10 }}>
                            <Copy size={11} strong muted>
                                资料
                            </Copy>
                        </View>
                        <View
                            style={{
                                paddingHorizontal: 10,
                                paddingVertical: 5,
                                gap: 9,
                            }}
                        >
                            <Copy size={11} muted lines={2}>
                                准原生应用的秩序
                            </Copy>
                            <Copy size={11} muted lines={2}>
                                插件与宿主之间
                            </Copy>
                        </View>
                    </View>
                </View>
            </Scroll>
            <Rule />
            <View style={{ padding: 13 }}>
                <Copy size={11} muted lines={2}>
                    引用与个人观点分开 · {notesSource()}
                </Copy>
            </View>
        </View>
    );
}

function NotesList(props: { compact: boolean }) {
    const list = () => visibleNotes();
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    height: 55,
                    paddingHorizontal: 18,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                    backgroundColor: ink("surface-base"),
                }}
            >
                <If when={props.compact}>
                    <Tool
                        label="显示想法导航"
                        icon="list"
                        text="导航"
                        size="touch"
                        onPress={() => notesNavigation.set(true)}
                    />
                </If>
                <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                    <Copy strong>{notesScope()}</Copy>
                    <Copy size={11} muted>
                        {list().length} 条想法
                    </Copy>
                    <Copy size={10} muted>
                        {notesSource()} · {notesSourceDetail()}
                    </Copy>
                </View>
                <If when={!props.compact}>
                    <View style={{ width: 240 }}>
                        <Input
                            ref={(node) => {
                                notesSearchRef = node;
                            }}
                            accessibilityLabel="搜索想法、引文或资料"
                            value={notesQuery()}
                            onChange={({ value }) => notesQuery.set(value)}
                            placeholder="搜索想法、引文或资料"
                            style={{
                                height: 32,
                                paddingHorizontal: 10,
                                fontSize: 13,
                                color: ink("text-primary"),
                                backgroundColor: ink("surface-inset"),
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                borderRadius: 7,
                            }}
                            focusStyle={focus()}
                        />
                    </View>
                </If>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View style={{ padding: 18, gap: 10 }}>
                    <Each of={list()} key={(note) => note.id}>
                        {(note) => {
                            const selected = () => notesSelected() === note().id;
                            return (

                                        <View
                                            accessibilityLabel={`《${note().document}》${noteLocationLabel(note())}：${note().title}`}
                                            onContextMenu={(event) => {
                                                selectNote(note());
                                                void DesktopMenu.popup(
                                                    noteMenuItems(note()),
                                                    event.pagePosition,
                                                );
                                            }}
                                            onPress={(event) => {
                                                if (event.clickCount === 2) {
                                                    openNoteSource(note());
                                                } else {
                                                    selectNote(note());
                                                }
                                            }}
                                            style={{
                                                padding: 15,
                                                gap: 10,
                                                borderWidth: 1,
                                                borderColor: ink(
                                                    selected()
                                                        ? "text-accent"
                                                        : "border-subtle",
                                                ),
                                                borderLeftWidth: selected() ? 3 : 1,
                                                borderRadius: 9,
                                                borderCurve: "continuous",
                                                backgroundColor: ink(
                                                    selected()
                                                        ? "surface-selected"
                                                        : "surface-preview",
                                                ),
                                                alignItems: "stretch",
                                            }}
                                            hoverStyle={{
                                                borderColor: ink("border-control"),
                                                backgroundColor: ink("surface-hover"),
                                            }}
                                            focusStyle={focus()}
                                        >
                                            <View
                                                style={{
                                                    flexDirection: "row",
                                                    alignItems: "center",
                                                    gap: 9,
                                                }}
                                            >
                                                <Badge icon="pen" size={28} />
                                                <View
                                                    style={{
                                                        flex: 1,
                                                        minWidth: 0,
                                                        gap: 2,
                                                    }}
                                                >
                                                    <Copy strong>
                                                        {note().title}
                                                    </Copy>
                                                    <Copy size={11} muted>
                                                        {note().document} ·{" "}
                                                        {noteLocationLabel(
                                                            note(),
                                                        )}
                                                    </Copy>
                                                </View>
                                                <If
                                                    when={noteTruncated(note())}
                                                >
                                                    <Copy size={11} muted>
                                                        完整内容
                                                    </Copy>
                                                </If>
                                                <If
                                                    when={
                                                        note().missingOriginal
                                                    }
                                                >
                                                    <Copy size={11} muted>
                                                        原件缺失
                                                    </Copy>
                                                </If>
                                                <If when={note().documentId !== undefined}>
                                                    <Copy size={11} muted>
                                                        可回源
                                                    </Copy>
                                                </If>
                                            </View>
                                            <View
                                                style={{
                                                    flexDirection: "row",
                                                    gap: 12,
                                                }}
                                            >
                                                <View
                                                    style={{
                                                        width: 3,
                                                        borderRadius: 2,
                                                        backgroundColor: ink(
                                                            "text-accent",
                                                        ),
                                                    }}
                                                />
                                                <View
                                                    style={{
                                                        flex: 1,
                                                        minWidth: 0,
                                                        paddingVertical: 2,
                                                    }}
                                                >
                                                    <Copy lines={2}>
                                                        {note().quote}
                                                    </Copy>
                                                </View>
                                            </View>
                                            <Copy size={12} muted lines={2}>
                                                {note().note}
                                            </Copy>
                                            <View
                                                style={{
                                                    flexDirection: "row",
                                                    alignItems: "center",
                                                }}
                                            >
                                                <Copy size={11} muted>
                                                    {note().date}
                                                </Copy>
                                                <View style={{ flex: 1 }} />
                                                <If when={note().starred}>
                                                    <Glyph
                                                        name="book"
                                                        size={13}
                                                        color={ink(
                                                            "text-accent",
                                                        )}
                                                    />
                                                </If>
                                            </View>
                                        </View>                            );
                        }}
                    </Each>
                    <If when={list().length === 0}>
                        <View style={{ padding: 28, gap: 8 }}>
                            <Copy strong>
                                {notesQuery().trim()
                                    ? "没有匹配的想法"
                                    : notesScope() === "当前资料"
                                      ? "当前资料还没有想法"
                                      : "还没有保存的想法"}
                            </Copy>
                            <Copy size={12} muted>
                                {notesQuery().trim()
                                    ? "清除搜索或切换分类后再试。"
                                    : "在正文中选文并写下想法，保存后会出现在这里。"}
                            </Copy>
                            <If when={!!notesQuery().trim()}>
                                <View style={{ flexDirection: "row" }}>
                                    <TextButton
                                        label="清除搜索"
                                        filled
                                        size={props.compact ? "touch" : "compact"}
                                        onPress={() => notesQuery.set("")}
                                    />
                                </View>
                            </If>
                        </View>
                    </If>
                </View>
            </Scroll>
        </View>
    );
}

function NoteDetail(props: { compact: boolean }) {
    const note = () => selectedNote();
    const canOpen = () =>
        !!note() &&
        !note()!.missingOriginal &&
        (note()!.documentId !== undefined ||
            note()!.scope === "当前资料");
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    minHeight: 55,
                    paddingHorizontal: props.compact ? 12 : 16,
                    paddingVertical: props.compact ? 6 : 0,
                    flexDirection: "row",
                    alignItems: "center",
                    flexWrap: "wrap",
                    gap: 9,
                }}
            >
                <Tool
                    label="返回想法列表"
                    icon="back"
                    text="想法列表"
                    size={props.compact ? "touch" : "regular"}
                    onPress={() => {
                        notesSelected.set("");
                        notesSelection.set("");
                        notesCopyStatus.set("");
                    }}
                />
                <View style={{ flex: 1, minWidth: 80 }}>
                    <Copy strong>{note()?.document}</Copy>
                    <Copy size={11} muted>
                        {noteLocationLabel(note())} · 引用与个人观点分开
                    </Copy>
                </View>
                <Tool
                    label={
                        canOpen()
                            ? "打开所在页"
                            : note()?.missingOriginal
                                ? "原件缺失，无法定位"
                                : "来源信息不足"
                    }
                    text="打开所在页"
                    icon="book"
                    size={props.compact ? "touch" : "regular"}
                    disabled={!canOpen()}
                    onPress={() => {
                        if (note()) openNoteSource(note()!);
                    }}
                />
                <Tool
                    label="编辑想法"
                    icon="edit"
                    text="编辑想法"
                    size={props.compact ? "touch" : "regular"}
                    disabled={!note() || notesSaving()}
                    onPress={() => {
                        if (note()) startEditingNote(note()!);
                    }}
                />
                <Tool
                    label="删除记录"
                    icon="trash"
                    text="删除记录"
                    size={props.compact ? "touch" : "regular"}
                    disabled={!note() || notesSaving()}
                    onPress={() => {
                        if (note()) void deleteNoteRecord(note()!);
                    }}
                />
                <Menu
                    trigger={
                        <Tool
                            label="更多想法操作"
                            icon="more"
                            text="更多"
                            size={props.compact ? "touch" : "regular"}
                        />
                    }
                    popupStyle={menuPopup()}
                >
                    <MenuItem
                        style={menuItem()}
                        onSelect={() => note() && noteCopy(note()!.quote, "引文")}
                    >
                        <Copy>复制引文</Copy>
                    </MenuItem>
                    <MenuItem
                        style={menuItem()}
                        onSelect={() =>
                            note() && noteCopy(noteBodyText(note()!), "想法")
                        }
                    >
                        <Copy>复制想法</Copy>
                    </MenuItem>
                    <MenuItem
                        style={menuItem()}
                        onSelect={() =>
                            note() && noteCopy(noteCitation(note()!), "引用")
                        }
                    >
                        <Copy>复制引用</Copy>
                    </MenuItem>
                    <MenuSeparator />
                    <MenuItem
                        style={menuItem()}
                        disabled={!note() || !noteTruncated(note()!)}
                        onSelect={() => note() && loadNoteBody(note()!)}
                    >
                        <Copy>读取完整内容</Copy>
                    </MenuItem>
                    <MenuSeparator />
                    <MenuItem
                        style={menuItem()}
                        disabled={!note() || notesSaving()}
                        onSelect={() => note() && startEditingNote(note()!)}
                    >
                        <Copy>编辑想法</Copy>
                    </MenuItem>
                    <MenuItem
                        style={menuItem()}
                        disabled={!note() || notesSaving()}
                        onSelect={() =>
                            note() && void deleteNoteRecord(note()!)
                        }
                    >
                        <Copy>删除记录</Copy>
                    </MenuItem>
                </Menu>
            </View>
            <Rule />
            <If when={!!notesSelection()}>
                <View
                    style={{
                        paddingHorizontal: 18,
                        paddingVertical: 8,
                        flexDirection: "row",
                        alignItems: "center",
                        gap: 8,
                        backgroundColor: ink("surface-toolbar"),
                        borderBottomWidth: 1,
                        borderColor: ink("border-subtle"),
                    }}
                >
                    <Copy size={11} muted>
                        已选取 {notesSelection().length} 个字符
                    </Copy>
                    <View style={{ flex: 1 }} />
                    <TextButton
                        label="复制选中"
                        accessibilityLabel="复制选中文本"
                        filled
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => noteCopy(notesSelection(), "选中内容")}
                    />
                    <TextButton
                        label="复制为引用"
                        accessibilityLabel="把选中文本复制为引用"
                        filled
                        size={props.compact ? "touch" : "compact"}
                        onPress={() =>
                            note() &&
                            noteCopy(
                                `> ${notesSelection()}\n\n— 《${note()!.document}》${noteLocationLabel(note()!)}`,
                                "引用",
                            )
                        }
                    />
                    <TextButton
                        label="取消选择"
                        accessibilityLabel="取消文本选取"
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => notesSelection.set("")}
                    />
                </View>
            </If>
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        width: "100%",
                        maxWidth: 720,
                        alignSelf: "center",
                        paddingHorizontal: 34,
                        paddingTop: 34,
                        paddingBottom: 70,
                        gap: 18,
                    }}
                >
                    <Copy size={12} muted>
                        {note()?.date}
                    </Copy>
                    <Copy size={27} strong>
                        {note()?.title}
                    </Copy>
                    <View style={{ flexDirection: "row", gap: 14 }}>
                        <View
                            style={{
                                width: 3,
                                borderRadius: 2,
                                backgroundColor: ink("text-accent"),
                            }}
                        />
                        <View
                            style={{
                                flex: 1,
                                minWidth: 0,
                                paddingVertical: 4,
                            }}
                        >
                            <TextView
                                markdown={note()?.quote}
                                selectable
                                onSelectionChange={({ text }) =>
                                    notesSelection.set(text)
                                }
                            />
                        </View>
                    </View>
                    <Rule />
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 8,
                        }}
                    >
                        <Copy size={11} strong muted>
                            我的想法
                        </Copy>
                        <View style={{ flex: 1 }} />
                        <If when={notesEditingId() !== note()?.id}>
                            <TextButton
                                label="编辑想法"
                                size={props.compact ? "touch" : "compact"}
                                filled
                                disabled={!note() || notesSaving()}
                                onPress={() => {
                                    if (note()) startEditingNote(note()!);
                                }}
                            />
                            <TextButton
                                label="删除记录"
                                size={props.compact ? "touch" : "compact"}
                                disabled={!note() || notesSaving()}
                                onPress={() => {
                                    if (note()) void deleteNoteRecord(note()!);
                                }}
                            />
                        </If>
                    </View>
                    <If when={notesEditingId() === note()?.id}>
                        <View style={{ gap: 10 }}>
                            <Input
                                accessibilityLabel="编辑想法内容"
                                multiline
                                value={notesEditDraft()}
                                onChange={({ value }) =>
                                    notesEditDraft.set(value)
                                }
                                placeholder="输入修改后的想法"
                                style={{
                                    minHeight: 96,
                                    padding: 12,
                                    fontSize: 15,
                                    lineHeight: 25,
                                    color: ink("text-primary"),
                                    backgroundColor: ink("surface-inset"),
                                    borderWidth: 1,
                                    borderColor: ink("border-control"),
                                    borderRadius: 8,
                                }}
                                focusStyle={focus()}
                            />
                            <View
                                style={{
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 8,
                                }}
                            >
                                <Tool
                                    label="保存修改"
                                    text={
                                        notesSaving()
                                            ? "正在保存…"
                                            : "保存修改"
                                    }
                                    raised
                                    size={props.compact ? "touch" : "regular"}
                                    disabled={notesSaving()}
                                    onPress={() => {
                                        if (note())
                                            void saveEditedNote(note()!);
                                    }}
                                />
                                <TextButton
                                    label="取消"
                                    size={props.compact ? "touch" : "regular"}
                                    disabled={notesSaving()}
                                    onPress={() => notesEditingId.set("")}
                                />
                            </View>
                        </View>
                    </If>
                    <If when={notesEditingId() !== note()?.id}>
                        <TextView
                            markdown={noteBodyText(note()!)}
                            selectable
                            onSelectionChange={({ text }) =>
                                notesSelection.set(text)
                            }
                        />
                    </If>
                    <If
                        when={
                            !!note() &&
                            noteTruncated(note()!) &&
                            !notesBody()[note()!.id]
                        }
                    >
                        <View
                            style={{
                                padding: 12,
                                gap: 8,
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                borderRadius: 8,
                                backgroundColor: ink("surface-inset"),
                            }}
                        >
                            <Copy size={11} muted>
                                列表只显示开头，完整内容仍保存在本机。
                            </Copy>
                            <View style={{ flexDirection: "row" }}>
                                <TextButton
                                    label={
                                        notesBodyLoading() === note()!.id
                                            ? "正在读取…"
                                            : "读取完整内容"
                                    }
                                    accessibilityLabel="读取完整想法内容"
                                    raised
                                    size={props.compact ? "touch" : "compact"}
                                    disabled={notesBodyLoading() === note()!.id}
                                    onPress={() => loadNoteBody(note()!)}
                                />
                            </View>
                        </View>
                    </If>
                    <If when={!!notesCopyStatus()}>
                        <Copy size={11} muted>
                            {notesCopyStatus()}
                        </Copy>
                    </If>
                    <View
                        style={{
                            marginTop: 8,
                            paddingTop: 16,
                            borderTopWidth: 1,
                            borderColor: ink("border-subtle"),
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 10,
                        }}
                    >
                        <If when={note()?.missingOriginal}>
                            <Copy size={11} muted>
                                原件缺失，无法定位到这一页；引文与想法仍可复制。
                            </Copy>
                        </If>
                        <If
                            when={
                                !note()?.missingOriginal &&
                                note()?.documentId === undefined &&
                                note()?.scope !== "当前资料"
                            }
                        >
                            <Copy size={11} muted>
                                先打开《{note()?.document}》后可定位到这一页。
                            </Copy>
                        </If>
                        <If when={canOpen()}>
                            <Copy size={11} muted>
                                通过资料库插件打开已保存的来源页；不可用时会明确反馈。
                            </Copy>
                        </If>
                    </View>
                </View>
            </Scroll>
        </View>
    );
}

export function NotesWorkspace(props: { compact: boolean }) {
    effect(() => {
        if (
            !props.compact ||
            (!notesSelected() && !notesNavigation() && !notesSelection())
        )
            return;
        BackHandler.on("press", () => {
            if (notesSelection()) {
                notesSelection.set("");
                return true;
            }
            if (notesSelected()) {
                notesSelected.set("");
                notesCopyStatus.set("");
                return true;
            }
            if (notesNavigation()) {
                notesNavigation.set(false);
                return true;
            }
            return false;
        });
    });
    keymap.bind({
        up: () => moveNoteSelection(-1),
        down: () => moveNoteSelection(1),
        home: () => {
            const list = visibleNotes();
            if (list.length > 0) selectNote(list[0]);
        },
        end: () => {
            const list = visibleNotes();
            if (list.length > 0) selectNote(list[list.length - 1]);
        },
        enter: () => {
            const note = selectedNote();
            if (note) openNoteSource(note);
        },
        "mod+c": () => copyCurrentNote(),
        "mod+shift+c": () => {
            const note = selectedNote();
            if (note) noteCopy(noteCitation(note), "引用");
        },
        "mod+f": () => notesSearchRef?.focus(),
        "shift+f10": () => {
            const note = selectedNote();
            if (note) {
                selectNote(note);
                void DesktopMenu.popup(noteMenuItems(note));
            }
        },
        escape: () => {
            if (notesSelection()) {
                notesSelection.set("");
                return;
            }
            notesCopyStatus.set("");
            if (notesSelected()) notesSelected.set("");
        },
    });
    return (
        <View
            accessibilityLabel="想法插件工作区"
            style={{
                flex: 1,
                minWidth: 0,
                flexDirection: "row",
                backgroundColor: ink("surface-base"),
            }}
        >
            <If when={!props.compact}>
                <NotesSidebar compact={false} />
                <If when={!!selectedNote()}>
                    <NoteDetail compact={false} />
                </If>
                <If when={!selectedNote()}>
                    <NotesList compact={false} />
                </If>
            </If>
            <If when={props.compact && notesNavigation()}>
                <NotesSidebar compact={true} />
            </If>
            <If when={props.compact && !notesNavigation() && !selectedNote()}>
                <NotesList compact={true} />
            </If>
            <If when={props.compact && !notesNavigation() && !!selectedNote()}>
                <NoteDetail compact={true} />
            </If>
        </View>
    );
}
