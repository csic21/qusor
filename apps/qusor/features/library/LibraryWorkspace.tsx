import { Canvas, Clipboard, effect, Image, Picker, Scroll, signal, Storage, Text, View } from "nsh";
import { Button, Input } from "nsh/base";
import { BackHandler } from "nsh/mobile";
import { LibraryHost, VocabHost } from "qusor/pdf-surface";
import { Badge, BottomSheet, Copy, focus, ink, TextButton, Tool } from "../../ui";
import { loadReadingPosition, saveReadingPosition, type ReadingPosition } from "../../reading_position";
import { DEFAULT_EPUB_FONT_SIZE, DEFAULT_EPUB_LINE_HEIGHT, formatEpubPosition, loadReadingZoom, MAX_EPUB_FONT_SIZE, MAX_EPUB_LINE_HEIGHT, MIN_EPUB_FONT_SIZE, MIN_EPUB_LINE_HEIGHT, nextReadingZoom, normalizeEpubFontSize, normalizeEpubLineHeight, normalizeReadingZoom, parseEpubPosition, readingZoomLabel, saveReadingZoom, type EpubPosition } from "../../reading_view";
import { adjustSelectionEnd, adjustSelectionStart, boxesForRange, boxesForSavedRanges, glyphAt, parsePageGeometry, selectionBetween, selectionHandles, type PageGeometry, type SavedSelectionRange } from "../../reading_selection";
import { openPluginWorkspace } from "../../shell/opener";
import { refreshNotes } from "../notes/NotesWorkspace";
import {
    loadVocabDecks,
    vocabAnswer,
    vocabFailureText,
} from "../vocab/VocabWorkspace";

type LibraryRow = {
    document_id: number;
    title: string;
    page_count: number;
    original_bytes: number;
    commit_rev: number;
};
type EpubRow = {
    document_id: number;
    title: string;
    author: string;
    chapter_count: number;
    original_bytes: number;
    commit_rev: number;
};
type TocEntry = {
    title: string;
    index: number;
    depth: number;
};
type LibrarySourceTarget = {
    serial: number;
    documentId: number;
    pageIndex: number;
    title: string;
    quote: string;
    quoteTruncated: boolean;
    format?: "pdf" | "epub";
};

const libraryRows = signal<LibraryRow[]>([]);
const libraryEpubRows = signal<EpubRow[]>([]);
const libraryPositions = signal<Record<number, ReadingPosition>>({});
const libraryEpubPositions = signal<Record<number, EpubPosition>>({});
const libraryStatus = signal("正在读取本机资料库…");
const libraryNext = signal("");
const libraryImporting = signal(false);
export const libraryOpenDoc = signal<LibraryRow | null>(null);
const libraryOpenEpub = signal<EpubRow | null>(null);
const libraryViewMode = signal<"image" | "text">("image");
const libraryImmersive = signal(false);
const libraryTocOpen = signal(false);
const libraryTocEntries = signal<TocEntry[]>([]);
const libraryPageJumpInput = signal("");
const libraryNoteSheetOpen = signal(false);
const libraryPageIndex = signal(0);
const libraryPageText = signal("");
const libraryPageImage = signal<{
    documentId: number;
    pageIndex: number;
    url: string;
    width: number;
    height: number;
} | null>(null);
const libraryPageImageState = signal<"idle" | "loading" | "ready" | "failed">(
    "idle",
);
const libraryPageGeometry = signal<PageGeometry | null>(null);
const libraryPageStatus = signal("");
const libraryResumeNotice = signal("");
const librarySelection = signal("");
const librarySelectionStart = signal(0);
const librarySelectionScalarRange = signal<{
    start: number;
    end: number;
} | null>(null);
const librarySelectionBoxes = signal<
    [number, number, number, number][]
>([]);
const librarySavedSelections = signal<SavedSelectionRange[]>([]);
const librarySelectionNote = signal("");
const libraryZoom = signal(1);
const libraryReaderWidth = signal(0);
const librarySourceTarget = signal<LibrarySourceTarget | null>(null);
const librarySourceQuote = signal<{
    text: string;
    truncated: boolean;
} | null>(null);
const epubChapterIndex = signal(0);
const epubChapterTitle = signal("");
const epubChapterText = signal("");
const epubChapterStatus = signal("");
const epubSavedRanges = signal<SavedSelectionRange[]>([]);
const epubSelection = signal("");
const epubSelectionScalarStart = signal(0);
const epubSelectionNote = signal("");
const epubTocOpen = signal(false);
const epubTocEntries = signal<TocEntry[]>([]);
const epubTypographyOpen = signal(false);
const epubNoteSheetOpen = signal(false);
const epubFontSize = signal(DEFAULT_EPUB_FONT_SIZE);
const epubLineHeight = signal(DEFAULT_EPUB_LINE_HEIGHT);
const EPUB_TYPOGRAPHY_KEY = "qusor.library.epub-typography.v1";
const EPUB_POS_PREFIX = "qusor.library.epub-position.v1.";
let libraryBooted = false;
let nextLibrarySourceTarget = 0;
let handledLibrarySourceTarget = 0;
let libraryImageRequest = 0;
let librarySelectionAnchor: number | null = null;
let librarySelectionPointer: number | null = null;
let libraryDragMode: "new" | "start" | "end" = "new";
let libraryPageDisplay = { width: 0, height: 0 };
let librarySavedSelectionsRequest = 0;
let libraryZoomSaveRequest = 0;
const MAX_PDF_IMPORT_BYTES = 16 * 1024 * 1024;
const MAX_EPUB_IMPORT_BYTES = 16 * 1024 * 1024;

async function restoreEpubTypography() {
    try {
        const raw = await Storage.getItem(EPUB_TYPOGRAPHY_KEY);
        if (!raw) return;
        const parsed = JSON.parse(raw);
        epubFontSize.set(normalizeEpubFontSize(parsed?.fontSize));
        epubLineHeight.set(normalizeEpubLineHeight(parsed?.lineHeight));
    } catch (_) {}
}

async function saveEpubTypography(fontSize: number, lineHeight: number) {
    const nextFont = normalizeEpubFontSize(fontSize);
    const nextLine = normalizeEpubLineHeight(lineHeight);
    epubFontSize.set(nextFont);
    epubLineHeight.set(nextLine);
    try {
        await Storage.setItem(
            EPUB_TYPOGRAPHY_KEY,
            JSON.stringify({ fontSize: nextFont, lineHeight: nextLine }),
        );
    } catch (_) {}
}

async function loadEpubBookPosition(
    documentId: number,
    chapterCount: number,
): Promise<EpubPosition | null> {
    try {
        const raw = await Storage.getItem(`${EPUB_POS_PREFIX}${documentId}`);
        return parseEpubPosition(raw, chapterCount);
    } catch (_) {
        return null;
    }
}

async function saveEpubBookPosition(
    documentId: number,
    chapterIndex: number,
    scrollRatio = 0,
): Promise<EpubPosition> {
    const pos: EpubPosition = { chapterIndex, scrollRatio };
    try {
        await Storage.setItem(
            `${EPUB_POS_PREFIX}${documentId}`,
            formatEpubPosition(pos),
        );
    } catch (_) {}
    libraryEpubPositions.set({
        ...libraryEpubPositions(),
        [documentId]: pos,
    });
    return pos;
}

async function collectSelectionToVocab(
    rawText: string,
    contextText: string,
    sourceTitle: string,
    onStatus: (msg: string) => void,
) {
    const term = rawText.trim().split(/\s+/)[0]?.slice(0, 64) ?? "";
    if (!term) {
        onStatus("请先选择要收录的词条。");
        return;
    }
    const snippet = rawText.trim().slice(0, 180) || contextText.slice(0, 180);
    const payload = JSON.stringify({
        title: "阅读生词本",
        cards: [
            {
                term,
                meaning: `来自《${sourceTitle}》摘录`,
                example: snippet,
            },
        ],
    });
    try {
        vocabAnswer(await VocabHost.importWordbook(payload));
        void loadVocabDecks();
        onStatus(`已将「${term}」收录到「阅读生词本」词书。`);
    } catch (error) {
        onStatus(`收词未完成：${vocabFailureText(error)}`);
    }
}

async function restoreLibraryZoom() {
    const request = libraryZoomSaveRequest;
    const zoom = await loadReadingZoom();
    if (request === libraryZoomSaveRequest) libraryZoom.set(zoom);
}

async function changeLibraryZoom(zoom: number) {
    const normalized = normalizeReadingZoom(zoom);
    libraryZoom.set(normalized);
    const request = ++libraryZoomSaveRequest;
    const saved = await saveReadingZoom(normalized);
    if (request === libraryZoomSaveRequest) libraryZoom.set(saved);
}

function libraryRow(raw: any): LibraryRow | null {
    if (
        typeof raw?.document_id !== "number" ||
        typeof raw?.title !== "string" ||
        typeof raw?.page_count !== "number" ||
        typeof raw?.original_bytes !== "number" ||
        typeof raw?.commit_rev !== "number"
    ) {
        return null;
    }
    return raw;
}

function epubRow(raw: any): EpubRow | null {
    if (
        typeof raw?.document_id !== "number" ||
        typeof raw?.title !== "string" ||
        typeof raw?.chapter_count !== "number"
    ) {
        return null;
    }
    return {
        document_id: raw.document_id,
        title: raw.title,
        author: typeof raw.author === "string" ? raw.author : "",
        chapter_count: raw.chapter_count,
        original_bytes:
            typeof raw.original_bytes === "number" ? raw.original_bytes : 0,
        commit_rev: typeof raw.commit_rev === "number" ? raw.commit_rev : 0,
    };
}

async function refreshLibraryPositions(rows: LibraryRow[]) {
    const positions: Record<number, ReadingPosition> = {};
    await Promise.all(
        rows.map(async (row) => {
            const position = await loadReadingPosition(
                row.document_id,
                row.page_count,
            );
            if (position) positions[row.document_id] = position;
        }),
    );
    libraryPositions.set(positions);
}

async function refreshEpubPositions(rows: EpubRow[]) {
    const positions: Record<number, EpubPosition> = {};
    await Promise.all(
        rows.map(async (row) => {
            const pos = await loadEpubBookPosition(
                row.document_id,
                row.chapter_count,
            );
            if (pos) positions[row.document_id] = pos;
        }),
    );
    libraryEpubPositions.set(positions);
}

function libraryPositionOf(row: LibraryRow): ReadingPosition | null {
    return libraryPositions()[row.document_id] ?? null;
}

function libraryPositionLabel(row: LibraryRow): string {
    const position = libraryPositionOf(row);
    return position ? ` · 上次读到第 ${position.page + 1} 页` : "";
}

function epubPositionOf(row: EpubRow): EpubPosition | null {
    return libraryEpubPositions()[row.document_id] ?? null;
}

function epubPositionLabel(row: EpubRow): string {
    const pos = epubPositionOf(row);
    return pos ? ` · 上次读到第 ${pos.chapterIndex + 1} 章` : "";
}

export function openLibrarySource(
    target: Omit<LibrarySourceTarget, "serial">,
): boolean {
    const previous = librarySourceTarget();
    librarySourceTarget.set({
        ...target,
        serial: ++nextLibrarySourceTarget,
    });
    if (openPluginWorkspace("library_workspace")) return true;
    librarySourceTarget.set(previous);
    return false;
}

async function loadLibraryPage(cursor = "") {
    libraryStatus.set("正在读取本机资料库…");
    let pdfLoaded = false;
    let epubLoaded = false;
    try {
        const response = JSON.parse(await LibraryHost.page(cursor, 6));
        if (response.ok && Array.isArray(response.value?.documents)) {
            const rows: LibraryRow[] = [];
            for (const raw of response.value.documents) {
                const row = libraryRow(raw);
                if (row) rows.push(row);
            }
            libraryRows.set(rows);
            await refreshLibraryPositions(rows);
            libraryNext.set(
                typeof response.value.next_cursor === "string"
                    ? response.value.next_cursor
                    : "",
            );
            pdfLoaded = true;
        }
    } catch (_) {
        libraryRows.set([]);
        libraryNext.set("");
    }
    try {
        const host = LibraryHost as any;
        if (typeof host?.epubPage === "function") {
            const epubRaw = await host.epubPage("", 20);
            const epubRes = JSON.parse(epubRaw);
            if (epubRes?.ok && Array.isArray(epubRes.value?.items)) {
                const books: EpubRow[] = [];
                for (const raw of epubRes.value.items) {
                    const row = epubRow(raw);
                    if (row) books.push(row);
                }
                libraryEpubRows.set(books);
                await refreshEpubPositions(books);
                epubLoaded = true;
            }
        }
    } catch (_) {
        libraryEpubRows.set([]);
    }
    const totalCount = libraryRows().length + libraryEpubRows().length;
    if (pdfLoaded || epubLoaded) {
        libraryStatus.set(
            totalCount > 0
                ? "本机 Core · 只显示资料元数据"
                : "本机 Core 还没有导入 PDF 或 EPUB 资料",
        );
    } else {
        libraryStatus.set("本机资料库暂不可用；已收录内容没有被删除");
    }
}

async function deleteLibraryDocument(
    documentId: number,
    format: "pdf" | "epub",
    title: string,
) {
    const host = LibraryHost as any;
    if (typeof host?.deleteDocument !== "function") {
        libraryStatus.set("当前宿主尚未启用资料删除接口。");
        return;
    }
    try {
        const raw = await host.deleteDocument(documentId, format);
        const res = JSON.parse(raw);
        if (!res?.ok) {
            throw new Error(String(res?.error ?? "删除失败"));
        }
        if (libraryOpenDoc()?.document_id === documentId && format === "pdf") {
            clearLibraryPageImage();
            libraryOpenDoc.set(null);
        }
        if (
            libraryOpenEpub()?.document_id === documentId &&
            format === "epub"
        ) {
            libraryOpenEpub.set(null);
        }
        await loadLibraryPage();
        libraryStatus.set(`已从本机资料库移除《${title}》。`);
    } catch (error) {
        libraryStatus.set(
            `删除《${title}》未完成：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    }
}

function clearLibraryPageImage() {
    ++libraryImageRequest;
    const previous = libraryPageImage();
    libraryPageImage.set(null);
    libraryPageImageState.set("idle");
    libraryPageGeometry.set(null);
    libraryPageDisplay = { width: 0, height: 0 };
    librarySavedSelections.set([]);
    ++librarySavedSelectionsRequest;
    clearLibrarySelection();
    if (previous?.url) URL.revokeObjectURL(previous.url);
}

function clearLibrarySelection() {
    librarySelection.set("");
    librarySelectionStart.set(0);
    librarySelectionScalarRange.set(null);
    librarySelectionBoxes.set([]);
    librarySelectionAnchor = null;
    librarySelectionPointer = null;
    libraryDragMode = "new";
    libraryNoteSheetOpen.set(false);
}

function librarySelectionGeometry(
    displayX: number,
    displayY: number,
): { geometry: PageGeometry; glyph: number } | null {
    const geometry = libraryPageGeometry();
    if (!geometry) return null;
    const glyph = glyphAt(geometry, libraryPageDisplay, displayX, displayY);
    return glyph === null ? null : { geometry, glyph };
}

function hitLibrarySelectionHandle(
    displayX: number,
    displayY: number,
): "start" | "end" | null {
    const geometry = libraryPageGeometry();
    const range = librarySelectionScalarRange();
    if (
        !geometry ||
        !range ||
        libraryPageDisplay.width <= 0 ||
        libraryPageDisplay.height <= 0
    ) {
        return null;
    }
    const handles = selectionHandles(geometry, range.start, range.end);
    if (!handles) return null;
    const scaleX = libraryPageDisplay.width / geometry.width;
    const scaleY = libraryPageDisplay.height / geometry.height;
    const startX = handles.startBox[0] * scaleX;
    const startY = handles.startBox[1] * scaleY;
    const endX = (handles.endBox[0] + handles.endBox[2]) * scaleX;
    const endY = (handles.endBox[1] + handles.endBox[3]) * scaleY;
    const distStart = Math.hypot(displayX - startX, displayY - startY);
    const distEnd = Math.hypot(displayX - endX, displayY - endY);
    const threshold = 28;
    if (distStart <= threshold && distStart <= distEnd) return "start";
    if (distEnd <= threshold) return "end";
    return null;
}

function applyScalarRangeSelection(
    geometry: PageGeometry,
    start: number,
    end: number,
): boolean {
    const scalars = Array.from(libraryPageText());
    if (start < 0 || end <= start || end > scalars.length) return false;
    const text = scalars.slice(start, end).join("");
    if (!text.trim()) return false;
    librarySelection.set(text);
    librarySelectionStart.set(start);
    librarySelectionScalarRange.set({ start, end });
    librarySelectionBoxes.set(boxesForRange(geometry, start, end));
    return true;
}

function beginLibrarySelection(
    pointer: number,
    displayX: number,
    displayY: number,
): boolean {
    const handleHit = hitLibrarySelectionHandle(displayX, displayY);
    if (handleHit) {
        librarySelectionPointer = pointer;
        libraryDragMode = handleHit;
        return true;
    }
    const hit = librarySelectionGeometry(displayX, displayY);
    if (!hit) return false;
    librarySelectionPointer = pointer;
    librarySelectionAnchor = hit.glyph;
    libraryDragMode = "new";
    return selectLibraryGlyphs(hit.geometry, hit.glyph, hit.glyph);
}

function moveLibrarySelection(
    pointer: number,
    displayX: number,
    displayY: number,
) {
    if (librarySelectionPointer !== pointer) return;
    const geometry = libraryPageGeometry();
    const range = librarySelectionScalarRange();
    if (libraryDragMode === "start" && geometry && range) {
        const next = adjustSelectionStart(
            geometry,
            libraryPageDisplay,
            displayX,
            displayY,
            range.start,
            range.end,
        );
        applyScalarRangeSelection(geometry, next.start, next.end);
        return;
    }
    if (libraryDragMode === "end" && geometry && range) {
        const next = adjustSelectionEnd(
            geometry,
            libraryPageDisplay,
            displayX,
            displayY,
            range.start,
            range.end,
        );
        applyScalarRangeSelection(geometry, next.start, next.end);
        return;
    }
    if (librarySelectionAnchor === null) return;
    const hit = librarySelectionGeometry(displayX, displayY);
    if (hit) {
        selectLibraryGlyphs(
            hit.geometry,
            librarySelectionAnchor,
            hit.glyph,
        );
    }
}

function selectLibraryGlyphs(
    geometry: PageGeometry,
    first: number,
    last: number,
): boolean {
    const selection = selectionBetween(
        geometry,
        libraryPageText(),
        first,
        last,
    );
    if (!selection) return false;
    const end = selection.start + Array.from(selection.text).length;
    librarySelection.set(selection.text);
    librarySelectionStart.set(selection.start);
    librarySelectionScalarRange.set({ start: selection.start, end });
    librarySelectionBoxes.set(selection.boxes);
    return true;
}

function endLibrarySelection(pointer: number) {
    if (librarySelectionPointer === pointer) {
        librarySelectionPointer = null;
        librarySelectionAnchor = null;
        libraryDragMode = "new";
    }
}

async function openPdfToc(row: LibraryRow) {
    libraryPageJumpInput.set(String(libraryPageIndex() + 1));
    libraryTocOpen.set(true);
    const host = LibraryHost as any;
    if (typeof host?.pdfToc !== "function") {
        libraryTocEntries.set([]);
        return;
    }
    try {
        const raw = await host.pdfToc(row.document_id);
        const res = JSON.parse(raw);
        if (res?.ok && Array.isArray(res.value?.toc)) {
            libraryTocEntries.set(
                res.value.toc.map((item: any) => ({
                    title: String(item.title ?? ""),
                    index: Number(item.page_index ?? 0),
                    depth: Number(item.depth ?? 0),
                })),
            );
        } else {
            libraryTocEntries.set([]);
        }
    } catch (_) {
        libraryTocEntries.set([]);
    }
}

async function loadLibraryPageImage(row: LibraryRow, pageIndex: number) {
    const request = ++libraryImageRequest;
    const previous = libraryPageImage();
    if (previous?.url) URL.revokeObjectURL(previous.url);
    libraryPageImage.set(null);
    libraryPageImageState.set("loading");
    libraryPageGeometry.set(null);
    try {
        const frame = await LibraryHost.pageImage(row.document_id, pageIndex);
        if (request !== libraryImageRequest) return;
        const info = JSON.parse(frame.info);
        if (
            !info.ok ||
            !frame.file ||
            info.document_id !== row.document_id ||
            info.page_index !== pageIndex ||
            typeof info.width !== "number" ||
            typeof info.height !== "number"
        ) {
            throw Error(info.error ?? "PDF page rendering failed");
        }
        libraryPageImage.set({
            documentId: row.document_id,
            pageIndex,
            url: URL.createObjectURL(frame.file),
            width: info.width,
            height: info.height,
        });
        libraryPageGeometry.set(
            parsePageGeometry(
                frame.geometry,
                info.width,
                info.height,
            ),
        );
        libraryPageImageState.set("ready");
        libraryPageStatus.set(
            libraryPageText()
                ? "本机 Core · 原生页图 + 文字层"
                : "页图已加载；正在读取文字层…",
        );
        if (pageIndex + 1 < row.page_count) {
            void LibraryHost.prefetchPage(
                row.document_id,
                pageIndex + 1,
            ).catch(() => {});
        }
    } catch (error) {
        if (request === libraryImageRequest) {
            libraryPageImageState.set("failed");
            libraryPageStatus.set(
                `页图暂时无法渲染：${
                    error instanceof Error ? error.message : "未知错误"
                }。已可切换重排文字阅读。`,
            );
        }
    }
}

async function loadLibrarySavedSelections(
    row: LibraryRow,
    pageIndex: number,
) {
    const request = ++librarySavedSelectionsRequest;
    try {
        const response = JSON.parse(
            await LibraryHost.savedSelections(row.document_id, pageIndex),
        );
        if (
            request !== librarySavedSelectionsRequest ||
            !response.ok ||
            response.value?.document_id !== row.document_id ||
            response.value?.page_index !== pageIndex ||
            !Array.isArray(response.value?.ranges)
        ) {
            return;
        }
        librarySavedSelections.set(
            response.value.ranges.filter(
                (range: any) =>
                    Number.isSafeInteger(range?.capture_id) &&
                    Number.isSafeInteger(range?.start) &&
                    Number.isSafeInteger(range?.end) &&
                    range.start >= 0 &&
                    range.start < range.end,
            ),
        );
    } catch (_) {
        if (request === librarySavedSelectionsRequest) {
            librarySavedSelections.set([]);
        }
    }
}

async function loadLibraryText(row: LibraryRow, pageIndex: number) {
    libraryOpenEpub.set(null);
    libraryOpenDoc.set(row);
    libraryPageIndex.set(pageIndex);
    libraryPageText.set("");
    clearLibraryPageImage();
    clearLibrarySelection();
    librarySelectionNote.set("");
    librarySourceQuote.set(null);
    libraryResumeNotice.set("");
    libraryPageStatus.set("正在读取本机页图与文字…");
    void loadLibraryPageImage(row, pageIndex);
    void loadLibrarySavedSelections(row, pageIndex);
    let offset = 0;
    let text = "";
    try {
        for (let chunk = 0; chunk < 64; chunk += 1) {
            const response = JSON.parse(
                await LibraryHost.pageText(row.document_id, pageIndex, offset, 2048),
            );
            if (!response.ok || !response.value)
                throw Error(response.error ?? "page text failed");
            if (response.value.document_id !== row.document_id)
                throw Error("stale library document");
            text += String(response.value.text ?? "");
            if (response.value.next_offset == null) break;
            const next = Number(response.value.next_offset);
            if (!Number.isFinite(next) || next <= offset)
                throw Error("invalid page offset");
            offset = next;
        }
        libraryPageText.set(text);
        const imageState = libraryPageImageState();
        libraryPageStatus.set(
            text
                ? imageState === "ready"
                    ? "本机 Core · 原生页图 + 文字层"
                    : imageState === "failed"
                      ? "本机 Core · 文字层可用；页图暂未渲染"
                      : "本机 Core · 正在加载页图，文字层已可用"
                : "这一页没有可提取的文字层。",
        );
        const saved = await saveReadingPosition(
            row.document_id,
            pageIndex,
            row.page_count,
        );
        if (saved) {
            libraryPositions.set({
                ...libraryPositions(),
                [row.document_id]: saved,
            });
        }
    } catch (_) {
        libraryPageStatus.set("这一页的文字暂时读不到；页图或资料仍在本机。");
    }
}

async function importLibraryPdf() {
    if (libraryImporting()) return;
    libraryImporting.set(true);
    libraryStatus.set("请选择本地 PDF；原文件只写入本机 Core。");
    try {
        const picked = await Picker.pickFiles({
            multiple: false,
            filters: [{ name: "PDF 文档", extensions: ["pdf"] }],
        });
        if (picked.length === 0) {
            libraryStatus.set("已取消导入；当前资料库没有变化。");
            return;
        }
        const file = picked[0].file;
        if (file.size > MAX_PDF_IMPORT_BYTES) {
            throw new Error("PDF 不能超过 16 MiB");
        }
        const bytes = new Uint8Array(await file.arrayBuffer());
        const response = JSON.parse(
            await LibraryHost.importPdf(file.name, bytes),
        );
        if (!response.ok || !response.value) {
            throw new Error(response.error ?? "PDF import failed");
        }
        await loadLibraryPage();
        const documentId = Number(response.value.document_id);
        const imported =
            libraryRows().find((row) => row.document_id === documentId) ??
            ({
                document_id: documentId,
                title: String(response.value.title ?? file.name),
                page_count: Number(response.value.page_count),
                original_bytes: file.size,
                commit_rev: 0,
            } satisfies LibraryRow);
        if (
            Number.isSafeInteger(imported.document_id) &&
            imported.page_count > 0
        ) {
            await loadLibraryText(imported, 0);
            libraryResumeNotice.set("PDF 已写入本机 Core，并打开第一页。");
        } else {
            libraryStatus.set("PDF 已写入本机 Core；刷新列表后可阅读。");
        }
    } catch (error) {
        libraryStatus.set(
            `PDF 未导入：${
                error instanceof Error ? error.message : "未知错误"
            }。原资料库没有被删除。`,
        );
    } finally {
        libraryImporting.set(false);
    }
}

async function importLibraryEpub() {
    if (libraryImporting()) return;
    const host = LibraryHost as any;
    if (typeof host?.importEpub !== "function") {
        libraryStatus.set("当前宿主尚未启用 EPUB 导入接口。");
        return;
    }
    libraryImporting.set(true);
    libraryStatus.set("请选择本地 EPUB 书籍；原文件只写入本机 Core。");
    try {
        const picked = await Picker.pickFiles({
            multiple: false,
            filters: [{ name: "EPUB 书籍", extensions: ["epub"] }],
        });
        if (picked.length === 0) {
            libraryStatus.set("已取消导入；当前资料库没有变化。");
            return;
        }
        const file = picked[0].file;
        if (file.size > MAX_EPUB_IMPORT_BYTES) {
            throw new Error("EPUB 不能超过 16 MiB");
        }
        const bytes = new Uint8Array(await file.arrayBuffer());
        const raw = await host.importEpub(file.name, bytes);
        const response = JSON.parse(raw);
        if (!response?.ok || !response.value) {
            throw new Error(String(response?.error ?? "EPUB 导入失败"));
        }
        await loadLibraryPage();
        const documentId = Number(response.value.document_id);
        const imported =
            libraryEpubRows().find(
                (item) => item.document_id === documentId,
            ) ??
            ({
                document_id: documentId,
                title: String(response.value.title ?? file.name),
                author: String(response.value.author ?? ""),
                chapter_count: Number(response.value.chapter_count ?? 1),
                original_bytes: file.size,
                commit_rev: Number(response.value.commit_rev ?? 0),
            } satisfies EpubRow);
        if (
            Number.isSafeInteger(imported.document_id) &&
            imported.chapter_count > 0
        ) {
            await loadEpubChapterContent(imported, 0);
            libraryResumeNotice.set("EPUB 已写入本机 Core，并打开第一章。");
        } else {
            libraryStatus.set("EPUB 已写入本机 Core；刷新列表后可阅读。");
        }
    } catch (error) {
        libraryStatus.set(
            `EPUB 未导入：${
                error instanceof Error ? error.message : String(error)
            }。原资料库没有被删除。`,
        );
    } finally {
        libraryImporting.set(false);
    }
}

async function loadEpubSavedHighlights(book: EpubRow, chapterIndex: number) {
    const host = LibraryHost as any;
    if (typeof host?.epubSavedSelections !== "function") {
        epubSavedRanges.set([]);
        return;
    }
    try {
        const raw = await host.epubSavedSelections(
            book.document_id,
            chapterIndex,
        );
        const res = JSON.parse(raw);
        if (res?.ok && Array.isArray(res.value?.ranges)) {
            epubSavedRanges.set(
                res.value.ranges.filter(
                    (r: any) =>
                        Number.isSafeInteger(r?.capture_id) &&
                        Number.isSafeInteger(r?.start) &&
                        Number.isSafeInteger(r?.end) &&
                        r.start >= 0 &&
                        r.start < r.end,
                ),
            );
        } else {
            epubSavedRanges.set([]);
        }
    } catch (_) {
        epubSavedRanges.set([]);
    }
}

async function loadEpubChapterContent(book: EpubRow, chapterIndex: number) {
    clearLibraryPageImage();
    libraryOpenDoc.set(null);
    libraryOpenEpub.set(book);
    epubChapterIndex.set(chapterIndex);
    epubChapterTitle.set(`第 ${chapterIndex + 1} 章`);
    epubChapterText.set("");
    epubSelection.set("");
    epubSelectionScalarStart.set(0);
    epubSelectionNote.set("");
    epubNoteSheetOpen.set(false);
    librarySourceQuote.set(null);
    libraryResumeNotice.set("");
    epubChapterStatus.set("正在读取本机 EPUB 章节…");
    void loadEpubSavedHighlights(book, chapterIndex);
    const host = LibraryHost as any;
    if (typeof host?.epubChapter !== "function") {
        epubChapterStatus.set("当前宿主尚未启用 EPUB 章节读取接口。");
        return;
    }
    try {
        const raw = await host.epubChapter(book.document_id, chapterIndex);
        const res = JSON.parse(raw);
        if (!res?.ok || !res.value) {
            throw new Error(String(res?.error ?? "章节读取失败"));
        }
        const title = String(res.value.title ?? `第 ${chapterIndex + 1} 章`);
        const text = String(res.value.text ?? "");
        epubChapterTitle.set(title);
        epubChapterText.set(text);
        epubChapterStatus.set(
            `本机 Core · 原生重排排版 · 第 ${chapterIndex + 1}/${book.chapter_count} 章`,
        );
        await saveEpubBookPosition(book.document_id, chapterIndex, 0);
    } catch (error) {
        epubChapterStatus.set(
            `章节读取未完成：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    }
}

async function openEpubDocument(book: EpubRow) {
    const saved = await loadEpubBookPosition(
        book.document_id,
        book.chapter_count,
    );
    const chapter = saved ? saved.chapterIndex : 0;
    await loadEpubChapterContent(book, chapter);
    if (saved) {
        libraryResumeNotice.set(
            `已继续上次阅读 · 第 ${saved.chapterIndex + 1} 章`,
        );
    }
}

async function openEpubToc(book: EpubRow) {
    epubTocOpen.set(true);
    const host = LibraryHost as any;
    if (typeof host?.epubToc !== "function") {
        epubTocEntries.set([]);
        return;
    }
    try {
        const raw = await host.epubToc(book.document_id);
        const res = JSON.parse(raw);
        if (res?.ok && Array.isArray(res.value?.toc)) {
            epubTocEntries.set(
                res.value.toc.map((item: any) => ({
                    title: String(item.title ?? ""),
                    index: Number(item.chapter_index ?? 0),
                    depth: Number(item.depth ?? 0),
                })),
            );
        } else {
            epubTocEntries.set([]);
        }
    } catch (_) {
        epubTocEntries.set([]);
    }
}

async function captureEpubExcerpt(book: EpubRow, chapterIndex: number) {
    const selected = epubSelection().trim();
    if (!selected) return;
    const host = LibraryHost as any;
    if (typeof host?.captureEpubSelection !== "function") {
        epubChapterStatus.set("当前宿主尚未启用 EPUB 摘录保存接口。");
        return;
    }
    const start = epubSelectionScalarStart();
    const end = start + Array.from(selected).length;
    const note = epubSelectionNote();
    const operationId = `epub-${book.document_id}-${chapterIndex}-${Date.now().toString(36)}`;
    try {
        const raw = await host.captureEpubSelection(
            book.document_id,
            chapterIndex,
            start,
            end,
            note,
            operationId,
        );
        const res = JSON.parse(raw);
        if (!res?.ok) {
            throw new Error(String(res?.error ?? "保存失败"));
        }
        epubChapterStatus.set("EPUB 选文与想法已保存到本机 Core。");
        epubSelection.set("");
        epubSelectionNote.set("");
        epubNoteSheetOpen.set(false);
        refreshNotes();
        void loadEpubSavedHighlights(book, chapterIndex);
    } catch (error) {
        epubChapterStatus.set(
            `选文暂未保存：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    }
}

async function openLibraryDocument(row: LibraryRow) {
    libraryOpenEpub.set(null);
    libraryOpenDoc.set(row);
    libraryViewMode.set("image");
    libraryImmersive.set(false);
    libraryTocOpen.set(false);
    libraryPageIndex.set(0);
    libraryPageText.set("");
    clearLibraryPageImage();
    clearLibrarySelection();
    librarySelectionNote.set("");
    librarySourceQuote.set(null);
    libraryPageStatus.set("正在恢复上次阅读位置…");
    const position = await loadReadingPosition(
        row.document_id,
        row.page_count,
    );
    const page = position ? position.page : 0;
    await loadLibraryText(row, page);
    if (position) {
        libraryResumeNotice.set(
            `已继续上次阅读 · 第 ${position.page + 1} 页`,
        );
    }
}

async function openLibrarySourceTarget(target: LibrarySourceTarget) {
    libraryStatus.set(`正在定位《${target.title}》…`);
    if (target.format === "epub") {
        try {
            const host = LibraryHost as any;
            if (typeof host?.epubPage === "function") {
                const raw = await host.epubPage("", 50);
                const res = JSON.parse(raw);
                if (res?.ok && Array.isArray(res.value?.items)) {
                    const book = res.value.items
                        .map(epubRow)
                        .find(
                            (candidate: EpubRow | null) =>
                                candidate?.document_id === target.documentId,
                        );
                    if (book) {
                        const chapterIndex =
                            target.pageIndex < book.chapter_count
                                ? Math.max(0, target.pageIndex)
                                : 0;
                        await loadEpubChapterContent(book, chapterIndex);
                        librarySourceQuote.set({
                            text: target.quote,
                            truncated: target.quoteTruncated,
                        });
                        epubChapterStatus.set(
                            `已从想法定位到《${target.title}》第 ${chapterIndex + 1} 章。`,
                        );
                        return;
                    }
                }
            }
        } catch (_) {}
    }
    let cursor = "";
    const seen = new Set<string>();
    try {
        for (;;) {
            if (seen.has(cursor)) throw Error("library cursor cycle");
            seen.add(cursor);
            const response = JSON.parse(await LibraryHost.page(cursor, 6));
            if (!response.ok || !Array.isArray(response.value?.documents))
                throw Error(response.error ?? "library page failed");
            const row = response.value.documents
                .map(libraryRow)
                .find(
                    (candidate: LibraryRow | null) =>
                        candidate?.document_id === target.documentId,
                );
            if (row) {
                libraryViewMode.set("image");
                libraryImmersive.set(false);
                libraryTocOpen.set(false);
                if (target.pageIndex >= row.page_count) {
                    await loadLibraryText(row, 0);
                    libraryPageStatus.set(
                        `想法记录在第 ${target.pageIndex + 1} 页，但当前资料只有 ${row.page_count} 页；未定位到相似内容。`,
                    );
                    return;
                }
                await loadLibraryText(row, target.pageIndex);
                librarySourceQuote.set({
                    text: target.quote,
                    truncated: target.quoteTruncated,
                });
                libraryPageStatus.set(
                    `已从想法定位到《${target.title}》第 ${target.pageIndex + 1} 页。`,
                );
                return;
            }
            const next = response.value.next_cursor;
            if (typeof next !== "string" || !next) break;
            cursor = next;
        }
    } catch (_) {
        // Fall through to the normal list with an explicit locator failure.
    }
    await loadLibraryPage();
    libraryStatus.set(
        `没有找到《${target.title}》当前可读的原件；没有用相似内容代替。`,
    );
}

function utf16ToScalar(text: string, offset: number) {
    let units = 0;
    let scalars = 0;
    for (const character of text) {
        if (units >= offset) return scalars;
        units += character.length;
        scalars += 1;
    }
    return scalars;
}

async function captureLibrarySelection(row: LibraryRow, pageIndex: number) {
    const selected = librarySelection();
    if (!selected) return;
    const note = librarySelectionNote();
    const scalarRange = librarySelectionScalarRange();
    const start = scalarRange
        ? scalarRange.start
        : utf16ToScalar(libraryPageText(), librarySelectionStart());
    const end = scalarRange
        ? scalarRange.end
        : start + Array.from(selected).length;
    const operation = `library-${row.document_id}-${pageIndex}-${Date.now().toString(36)}`;
    try {
        const response = JSON.parse(
            await LibraryHost.captureSelection(
                row.document_id,
                pageIndex,
                start,
                end,
                note,
                operation,
            ),
        );
        if (!response.ok) throw Error(response.error ?? "capture failed");
        libraryPageStatus.set("选文已保存到本机 Core。");
        refreshNotes();
        void loadLibrarySavedSelections(row, pageIndex);
        clearLibrarySelection();
        librarySelectionNote.set("");
    } catch (_) {
        libraryPageStatus.set("选文暂未保存；原选文仍在当前页面。");
    }
}

function LibraryReader(props: { compact: boolean }) {
    const row = libraryOpenDoc();
    effect(() => {
        if (!row) return;
        BackHandler.on("press", () => {
            if (libraryNoteSheetOpen()) {
                libraryNoteSheetOpen.set(false);
                return true;
            }
            if (libraryTocOpen()) {
                libraryTocOpen.set(false);
                return true;
            }
            if (librarySelection()) {
                clearLibrarySelection();
                return true;
            }
            clearLibraryPageImage();
            libraryOpenDoc.set(null);
            return true;
        });
    });
    if (!row) return <View />;
    const pages = Math.max(1, row.page_count);
    const pageWidth = () =>
        Math.max(
            240,
            (libraryReaderWidth() || (props.compact ? 330 : 640)) *
                libraryZoom(),
        );
    const showTextMode = () =>
        libraryViewMode() === "text" || libraryPageImageState() === "failed";
    return (
        <View
            accessibilityLabel="资料库阅读视图"
            style={{
                flex: 1,
                minWidth: 0,
                backgroundColor: ink("surface-base"),
            }}
        >
            <View
                style={{
                    minHeight: props.compact ? 48 : 52,
                    paddingHorizontal: props.compact ? 10 : 18,
                    paddingVertical: 6,
                    flexDirection: "row",
                    alignItems: "center",
                    flexWrap: "wrap",
                    gap: 6,
                    backgroundColor: ink("surface-toolbar"),
                    borderBottomWidth: 1,
                    borderColor: ink("border-subtle"),
                }}
            >
                <Tool
                    label="返回资料库列表"
                    icon="back"
                    text="资料库"
                    size={props.compact ? "touch" : "regular"}
                    onPress={() => {
                        clearLibraryPageImage();
                        libraryOpenDoc.set(null);
                    }}
                />
                <View style={{ flex: 1, minWidth: 80 }}>
                    <Copy size={props.compact ? 13 : 15} strong>
                        {row.title}
                    </Copy>
                </View>
                <Tool
                    label="上一页"
                    icon="back"
                    size={props.compact ? "touch" : "compact"}
                    disabled={libraryPageIndex() === 0}
                    onPress={() =>
                        void loadLibraryText(row, libraryPageIndex() - 1)
                    }
                />
                <TextButton
                    label={`${libraryPageIndex() + 1} / ${pages}`}
                    accessibilityLabel="打开目录与页码跳转"
                    size={props.compact ? "touch" : "compact"}
                    filled
                    onPress={() => void openPdfToc(row)}
                />
                <Tool
                    label="下一页"
                    icon="forward"
                    size={props.compact ? "touch" : "compact"}
                    disabled={libraryPageIndex() + 1 >= pages}
                    onPress={() =>
                        void loadLibraryText(row, libraryPageIndex() + 1)
                    }
                />
                <Tool
                    label="目录"
                    icon="list"
                    text="目录"
                    size={props.compact ? "touch" : "compact"}
                    onPress={() => void openPdfToc(row)}
                />
                <TextButton
                    label={
                        libraryViewMode() === "image" ? "重排文字" : "页图视图"
                    }
                    accessibilityLabel="切换页图与重排文字视图"
                    size={props.compact ? "touch" : "compact"}
                    raised
                    onPress={() =>
                        libraryViewMode.set(
                            libraryViewMode() === "image" ? "text" : "image",
                        )
                    }
                />
                <TextButton
                    label={libraryImmersive() ? "退出沉浸" : "沉浸"}
                    size={props.compact ? "touch" : "compact"}
                    filled={!libraryImmersive()}
                    raised={libraryImmersive()}
                    onPress={() => libraryImmersive.set(!libraryImmersive())}
                />
            </View>
            <If when={!libraryImmersive()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 12 : 18,
                        paddingVertical: 6,
                        flexDirection: "row",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: 8,
                        backgroundColor: ink("surface-inset"),
                        borderBottomWidth: 1,
                        borderColor: ink("border-subtle"),
                    }}
                >
                    <If when={!showTextMode()}>
                        <Tool
                            label="缩小资料页图"
                            text="−"
                            size={props.compact ? "touch" : "compact"}
                            disabled={libraryZoom() <= 1}
                            onPress={() =>
                                void changeLibraryZoom(
                                    nextReadingZoom(libraryZoom(), -1),
                                )
                            }
                        />
                        <Copy size={11} muted>
                            {readingZoomLabel(libraryZoom())}
                        </Copy>
                        <Tool
                            label="放大资料页图"
                            text="＋"
                            size={props.compact ? "touch" : "compact"}
                            disabled={libraryZoom() >= 2}
                            onPress={() =>
                                void changeLibraryZoom(
                                    nextReadingZoom(libraryZoom(), 1),
                                )
                            }
                        />
                        <Tool
                            label="资料页图适合宽度"
                            text="适合宽度"
                            size={props.compact ? "touch" : "compact"}
                            disabled={libraryZoom() <= 1}
                            onPress={() => void changeLibraryZoom(1)}
                        />
                    </If>
                    <View style={{ flex: 1, minWidth: 120, gap: 2 }}>
                        <Copy size={11} muted>
                            {libraryPageStatus()}
                        </Copy>
                        <If when={!!libraryResumeNotice()}>
                            <Copy size={11} muted>
                                {libraryResumeNotice()}
                            </Copy>
                        </If>
                    </View>
                </View>
            </If>
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        padding: props.compact ? 12 : 28,
                        maxWidth: 900,
                        width: "100%",
                        alignSelf: "center",
                        gap: 14,
                    }}
                >
                    <If when={!!librarySourceQuote()}>
                        <View
                            style={{
                                gap: 4,
                                padding: 12,
                                borderRadius: 8,
                                borderLeftWidth: 3,
                                borderColor: ink("text-accent"),
                                backgroundColor: ink("surface-inset"),
                            }}
                        >
                            <Copy size={11} muted>
                                从想法回到此页 ·{" "}
                                {librarySourceQuote()?.truncated
                                    ? "引文预览"
                                    : "保存的引文"}
                            </Copy>
                            <Copy size={13} lines={3}>
                                {librarySourceQuote()?.text}
                            </Copy>
                        </View>
                    </If>
                    <If when={!showTextMode() && !!libraryPageImage()}>
                        <View
                            style={{
                                width: "100%",
                                maxWidth: 780,
                                alignSelf: "center",
                                padding: props.compact ? 4 : 10,
                                borderRadius: 8,
                                backgroundColor: ink("surface-inset"),
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                            }}
                            onLayout={({ bounds }) => {
                                const pad = props.compact ? 8 : 20;
                                const width = Math.max(
                                    0,
                                    bounds.size.width - pad,
                                );
                                if (
                                    width > 0 &&
                                    Math.abs(width - libraryReaderWidth()) > 0.5
                                ) {
                                    libraryReaderWidth.set(width);
                                }
                            }}
                        >
                            <Scroll
                                focusable={false}
                                horizontal
                                onScroll={() => {
                                    if (librarySelectionPointer !== null) {
                                        clearLibrarySelection();
                                    }
                                }}
                                style={{ width: "100%" }}
                                contentStyle={{
                                    minWidth: "100%",
                                    justifyContent: "center",
                                }}
                            >
                                <View
                                    style={{
                                        width: pageWidth(),
                                        aspectRatio:
                                            libraryPageImage()!.width /
                                            libraryPageImage()!.height,
                                        backgroundColor: ink("surface-base"),
                                        position: "relative",
                                    }}
                                >
                                    <Image
                                        accessibilityLabel={`《${row.title}》第 ${
                                            libraryPageIndex() + 1
                                        } 页原生页图`}
                                        src={libraryPageImage()!.url}
                                        fadeDuration={0}
                                        style={{
                                            width: "100%",
                                            height: "100%",
                                            objectFit: "contain",
                                        }}
                                    />
                                    <Canvas
                                        accessibilityLabel="页图选文层"
                                        focusable={false}
                                        onStartShouldSetResponder={(event) =>
                                            hitLibrarySelectionHandle(
                                                event.locationX,
                                                event.locationY,
                                            ) !== null ||
                                            librarySelectionGeometry(
                                                event.locationX,
                                                event.locationY,
                                            ) !== null
                                        }
                                        onResponderGrant={(event) => {
                                            librarySelectionNote.set("");
                                            beginLibrarySelection(
                                                event.identifier,
                                                event.locationX,
                                                event.locationY,
                                            );
                                        }}
                                        onResponderMove={(event) =>
                                            moveLibrarySelection(
                                                event.identifier,
                                                event.locationX,
                                                event.locationY,
                                            )
                                        }
                                        onResponderRelease={(event) =>
                                            endLibrarySelection(
                                                event.identifier,
                                            )
                                        }
                                        onResponderTerminate={(event) =>
                                            endLibrarySelection(
                                                event.identifier,
                                            )
                                        }
                                        onResponderTerminationRequest={() =>
                                            true
                                        }
                                        draw={(context, size) => {
                                            const geometry =
                                                libraryPageGeometry();
                                            if (!geometry) return;
                                            libraryPageDisplay = {
                                                width: size.width,
                                                height: size.height,
                                            };
                                            const scaleX =
                                                size.width / geometry.width;
                                            const scaleY =
                                                size.height / geometry.height;
                                            context.fillStyle = ink(
                                                "pdf-saved-selection",
                                            );
                                            for (const [x, y, width, height] of
                                                boxesForSavedRanges(
                                                    geometry,
                                                    librarySavedSelections(),
                                                )) {
                                                context.fillRect(
                                                    x * scaleX,
                                                    y * scaleY,
                                                    width * scaleX,
                                                    height * scaleY,
                                                );
                                            }
                                            context.fillStyle =
                                                ink("pdf-selection");
                                            for (const [x, y, width, height] of
                                                librarySelectionBoxes()) {
                                                context.fillRect(
                                                    x * scaleX,
                                                    y * scaleY,
                                                    width * scaleX,
                                                    height * scaleY,
                                                );
                                            }
                                            const range =
                                                librarySelectionScalarRange();
                                            if (range) {
                                                const handles =
                                                    selectionHandles(
                                                        geometry,
                                                        range.start,
                                                        range.end,
                                                    );
                                                if (handles) {
                                                    const [sx, sy, , sh] =
                                                        handles.startBox;
                                                    const [ex, ey, ew, eh] =
                                                        handles.endBox;
                                                    const hx1 = sx * scaleX;
                                                    const hy1 = sy * scaleY;
                                                    const hx2 =
                                                        (ex + ew) * scaleX;
                                                    const hy2 =
                                                        (ey + eh) * scaleY;
                                                    context.fillStyle =
                                                        ink("text-accent");
                                                    context.fillRect(
                                                        hx1 - 1,
                                                        hy1,
                                                        2,
                                                        sh * scaleY,
                                                    );
                                                    context.beginPath();
                                                    context.arc(
                                                        hx1,
                                                        hy1,
                                                        6,
                                                        0,
                                                        Math.PI * 2,
                                                    );
                                                    context.fill();
                                                    context.fillRect(
                                                        hx2 - 1,
                                                        ey * scaleY,
                                                        2,
                                                        eh * scaleY,
                                                    );
                                                    context.beginPath();
                                                    context.arc(
                                                        hx2,
                                                        hy2,
                                                        6,
                                                        0,
                                                        Math.PI * 2,
                                                    );
                                                    context.fill();
                                                }
                                            }
                                        }}
                                        style={{
                                            position: "absolute",
                                            left: 0,
                                            top: 0,
                                            width: "100%",
                                            height: "100%",
                                        }}
                                    />
                                </View>
                            </Scroll>
                        </View>
                    </If>
                    <If when={!showTextMode() && !libraryPageImage()}>
                        <View
                            style={{
                                padding: 28,
                                alignItems: "center",
                                gap: 10,
                            }}
                        >
                            <Copy muted>{libraryPageStatus()}</Copy>
                            <TextButton
                                label="切换到重排文字阅读"
                                size={props.compact ? "touch" : "regular"}
                                raised
                                onPress={() => libraryViewMode.set("text")}
                            />
                        </View>
                    </If>
                    <If when={showTextMode()}>
                        <View
                            selectable
                            accessibilityLabel="可选择的本页文字"
                            onSelectionChange={({ text, start }) => {
                                if (!text) return;
                                const startOffset = start ?? 0;
                                const scalarStart = utf16ToScalar(
                                    libraryPageText(),
                                    startOffset,
                                );
                                librarySelection.set(text);
                                librarySelectionStart.set(startOffset);
                                librarySelectionScalarRange.set({
                                    start: scalarStart,
                                    end:
                                        scalarStart +
                                        Array.from(text).length,
                                });
                            }}
                            style={{
                                padding: props.compact ? 16 : 24,
                                borderRadius: 10,
                                borderCurve: "continuous",
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                backgroundColor: ink("surface-preview"),
                            }}
                        >
                            <Text
                                style={{
                                    fontSize: 16,
                                    lineHeight: 29,
                                    color: ink("text-primary"),
                                    userSelect: "text",
                                }}
                            >
                                {libraryPageText() ||
                                    "本页暂无可提取的文字层。"}
                            </Text>
                        </View>
                    </If>
                </View>
            </Scroll>
            <If when={!!librarySelection()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 12 : 20,
                        paddingVertical: 10,
                        flexDirection: "row",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: 8,
                        backgroundColor: ink("surface-floating"),
                        borderTopWidth: 1,
                        borderColor: ink("border-control"),
                    }}
                >
                    <Copy size={12} strong>
                        {`已选 ${Array.from(librarySelection()).length} 字`}
                    </Copy>
                    <View style={{ flex: 1 }} />
                    <Tool
                        label="复制当前选文"
                        text="复制"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void Clipboard.writeText(librarySelection())
                        }
                    />
                    <Tool
                        label="保存当前选文和想法到 Core"
                        text="保存摘录"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void captureLibrarySelection(
                                row,
                                libraryPageIndex(),
                            )
                        }
                    />
                    <Tool
                        label="写想法并保存"
                        icon="pen"
                        text="写想法并保存"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() => libraryNoteSheetOpen.set(true)}
                    />
                    <Tool
                        label="收词到词书"
                        icon="book"
                        text="收词到词书"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void collectSelectionToVocab(
                                librarySelection(),
                                libraryPageText(),
                                row.title,
                                (msg) => libraryPageStatus.set(msg),
                            )
                        }
                    />
                    <TextButton
                        label="取消"
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => {
                            clearLibrarySelection();
                            librarySelectionNote.set("");
                        }}
                    />
                </View>
            </If>
            <BottomSheet
                open={libraryNoteSheetOpen()}
                onClose={() => libraryNoteSheetOpen.set(false)}
                title="写想法并保存到 Core"
                subtitle={`《${row.title}》第 ${libraryPageIndex() + 1} 页 · 已选 ${
                    Array.from(librarySelection()).length
                } 字`}
            >
                <View style={{ gap: 12 }}>
                    <View
                        style={{
                            padding: 10,
                            borderRadius: 8,
                            borderLeftWidth: 3,
                            borderColor: ink("text-accent"),
                            backgroundColor: ink("surface-inset"),
                        }}
                    >
                        <Copy size={12} lines={3}>
                            {librarySelection()}
                        </Copy>
                    </View>
                    <Input
                        accessibilityLabel="为摘录写下想法"
                        multiline
                        value={librarySelectionNote()}
                        onChange={({ value }) =>
                            librarySelectionNote.set(value)
                        }
                        placeholder="写下自己的想法（可选）"
                        style={{
                            minHeight: 88,
                            padding: 12,
                            fontSize: 15,
                            lineHeight: 24,
                            color: ink("text-primary"),
                            backgroundColor: ink("surface-base"),
                            borderWidth: 1,
                            borderColor: ink("border-control"),
                            borderRadius: 8,
                        }}
                        focusStyle={focus()}
                    />
                    <View
                        style={{
                            flexDirection: "row",
                            justifyContent: "flex-end",
                            gap: 10,
                        }}
                    >
                        <TextButton
                            label="取消"
                            size="touch"
                            onPress={() => libraryNoteSheetOpen.set(false)}
                        />
                        <Tool
                            label="保存摘录与想法"
                            text="保存到 Core"
                            raised
                            size="touch"
                            onPress={() =>
                                void captureLibrarySelection(
                                    row,
                                    libraryPageIndex(),
                                )
                            }
                        />
                    </View>
                </View>
            </BottomSheet>
            <BottomSheet
                open={libraryTocOpen()}
                onClose={() => libraryTocOpen.set(false)}
                title="目录与页码跳转"
                subtitle={`《${row.title}》 · 共 ${pages} 页`}
            >
                <View style={{ gap: 12, maxHeight: 420 }}>
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 8,
                        }}
                    >
                        <Input
                            accessibilityLabel="输入跳转页码"
                            value={libraryPageJumpInput()}
                            onChange={({ value }) =>
                                libraryPageJumpInput.set(value)
                            }
                            placeholder={`页码 (1-${pages})`}
                            style={{
                                flex: 1,
                                height: props.compact ? 44 : 38,
                                paddingHorizontal: 10,
                                fontSize: 14,
                                color: ink("text-primary"),
                                backgroundColor: ink("surface-inset"),
                                borderWidth: 1,
                                borderColor: ink("border-control"),
                                borderRadius: 8,
                            }}
                            focusStyle={focus()}
                        />
                        <Tool
                            label="跳转到指定页码"
                            text="跳转"
                            raised
                            size={props.compact ? "touch" : "regular"}
                            onPress={() => {
                                const targetPage =
                                    Number.parseInt(
                                        libraryPageJumpInput(),
                                        10,
                                    ) - 1;
                                if (
                                    Number.isSafeInteger(targetPage) &&
                                    targetPage >= 0 &&
                                    targetPage < pages
                                ) {
                                    libraryTocOpen.set(false);
                                    void loadLibraryText(row, targetPage);
                                }
                            }}
                        />
                    </View>
                    <Scroll style={{ maxHeight: 320 }}>
                        <View style={{ gap: 6 }}>
                            <If when={libraryTocEntries().length === 0}>
                                <Copy size={12} muted>
                                    此 PDF 未内嵌书签目录，可使用上方页码跳转。
                                </Copy>
                            </If>
                            <Each
                                of={libraryTocEntries()}
                                key={(item) =>
                                    `pdf-toc-${item.index}-${item.title}`
                                }
                            >
                                {(item) => (
                                    <Button
                                        accessibilityLabel={`跳转到 ${item().title}`}
                                        onPress={() => {
                                            libraryTocOpen.set(false);
                                            void loadLibraryText(
                                                row,
                                                Math.min(
                                                    pages - 1,
                                                    Math.max(0, item().index),
                                                ),
                                            );
                                        }}
                                        style={{
                                            minHeight: props.compact ? 44 : 40,
                                            paddingLeft:
                                                10 + item().depth * 14,
                                            paddingRight: 10,
                                            flexDirection: "row",
                                            alignItems: "center",
                                            borderRadius: 7,
                                            backgroundColor: ink(
                                                libraryPageIndex() ===
                                                    item().index
                                                    ? "surface-selected"
                                                    : "transparent",
                                            ),
                                        }}
                                        hoverStyle={{
                                            backgroundColor:
                                                ink("surface-hover"),
                                        }}
                                        focusStyle={focus()}
                                    >
                                        <View style={{ flex: 1, minWidth: 0 }}>
                                            <Copy>{item().title}</Copy>
                                        </View>
                                        <Copy size={11} muted>
                                            {`第 ${item().index + 1} 页`}
                                        </Copy>
                                    </Button>
                                )}
                            </Each>
                        </View>
                    </Scroll>
                </View>
            </BottomSheet>
        </View>
    );
}

type EpubParagraph = {
    index: number;
    text: string;
    scalarStart: number;
    scalarEnd: number;
};

function splitEpubParagraphs(text: string): EpubParagraph[] {
    const lines = text.split("\n");
    const result: EpubParagraph[] = [];
    let cursor = 0;
    for (let idx = 0; idx < lines.length; idx += 1) {
        const line = lines[idx];
        const len = Array.from(line).length;
        const trimmed = line.trim();
        if (trimmed) {
            result.push({
                index: result.length,
                text: trimmed,
                scalarStart: cursor,
                scalarEnd: cursor + len,
            });
        }
        cursor += len + 1;
    }
    return result;
}

function EpubReader(props: { compact: boolean }) {
    const book = libraryOpenEpub();
    effect(() => {
        if (!book) return;
        BackHandler.on("press", () => {
            if (epubNoteSheetOpen()) {
                epubNoteSheetOpen.set(false);
                return true;
            }
            if (epubTocOpen()) {
                epubTocOpen.set(false);
                return true;
            }
            if (epubTypographyOpen()) {
                epubTypographyOpen.set(false);
                return true;
            }
            if (epubSelection()) {
                epubSelection.set("");
                return true;
            }
            libraryOpenEpub.set(null);
            return true;
        });
    });
    if (!book) return <View />;
    const chapters = Math.max(1, book.chapter_count);
    const paragraphs = () => splitEpubParagraphs(epubChapterText());
    return (
        <View
            accessibilityLabel="EPUB 阅读视图"
            style={{
                flex: 1,
                minWidth: 0,
                backgroundColor: ink("surface-base"),
            }}
        >
            <View
                style={{
                    minHeight: props.compact ? 48 : 52,
                    paddingHorizontal: props.compact ? 10 : 18,
                    paddingVertical: 6,
                    flexDirection: "row",
                    alignItems: "center",
                    flexWrap: "wrap",
                    gap: 6,
                    backgroundColor: ink("surface-toolbar"),
                    borderBottomWidth: 1,
                    borderColor: ink("border-subtle"),
                }}
            >
                <Tool
                    label="返回资料库列表"
                    icon="back"
                    text="资料库"
                    size={props.compact ? "touch" : "regular"}
                    onPress={() => libraryOpenEpub.set(null)}
                />
                <View style={{ flex: 1, minWidth: 80 }}>
                    <Copy size={props.compact ? 13 : 15} strong>
                        {book.title}
                    </Copy>
                    <Copy size={11} muted>
                        {epubChapterTitle()}
                    </Copy>
                </View>
                <Tool
                    label="上一章"
                    icon="back"
                    size={props.compact ? "touch" : "compact"}
                    disabled={epubChapterIndex() === 0}
                    onPress={() =>
                        void loadEpubChapterContent(
                            book,
                            epubChapterIndex() - 1,
                        )
                    }
                />
                <TextButton
                    label={`第 ${epubChapterIndex() + 1}/${chapters} 章`}
                    accessibilityLabel="打开章节目录"
                    size={props.compact ? "touch" : "compact"}
                    filled
                    onPress={() => void openEpubToc(book)}
                />
                <Tool
                    label="下一章"
                    icon="forward"
                    size={props.compact ? "touch" : "compact"}
                    disabled={epubChapterIndex() + 1 >= chapters}
                    onPress={() =>
                        void loadEpubChapterContent(
                            book,
                            epubChapterIndex() + 1,
                        )
                    }
                />
                <Tool
                    label="目录"
                    icon="list"
                    text="目录"
                    size={props.compact ? "touch" : "compact"}
                    onPress={() => void openEpubToc(book)}
                />
                <Tool
                    label="排版设置"
                    icon="sliders"
                    text="排版"
                    size={props.compact ? "touch" : "compact"}
                    onPress={() => epubTypographyOpen.set(true)}
                />
            </View>
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        padding: props.compact ? 16 : 36,
                        maxWidth: 740,
                        width: "100%",
                        alignSelf: "center",
                        gap: 16,
                    }}
                >
                    <Copy size={11} muted>
                        {epubChapterStatus()}
                    </Copy>
                    <If when={!!libraryResumeNotice()}>
                        <Copy size={11} muted>
                            {libraryResumeNotice()}
                        </Copy>
                    </If>
                    <If when={!!librarySourceQuote()}>
                        <View
                            style={{
                                gap: 4,
                                padding: 12,
                                borderRadius: 8,
                                borderLeftWidth: 3,
                                borderColor: ink("text-accent"),
                                backgroundColor: ink("surface-inset"),
                            }}
                        >
                            <Copy size={11} muted>
                                从想法定位到此章 · 保存的引文
                            </Copy>
                            <Copy size={13} lines={3}>
                                {librarySourceQuote()?.text}
                            </Copy>
                        </View>
                    </If>
                    <Text
                        style={{
                            fontSize: epubFontSize() + 5,
                            lineHeight: Math.round(
                                (epubFontSize() + 5) * epubLineHeight(),
                            ),
                            fontWeight: "600",
                            color: ink("text-primary"),
                        }}
                    >
                        {epubChapterTitle()}
                    </Text>
                    <Each
                        of={paragraphs()}
                        key={(para) =>
                            `epub-para-${epubChapterIndex()}-${para.index}`
                        }
                    >
                        {(para) => {
                            const quoteSnippet =
                                librarySourceQuote()?.text
                                    .trim()
                                    .slice(0, 24) ?? "";
                            const highlighted = () =>
                                epubSavedRanges().some(
                                    (r) =>
                                        r.start < para().scalarEnd &&
                                        r.end > para().scalarStart,
                                ) ||
                                (quoteSnippet.length > 0 &&
                                    para().text.includes(quoteSnippet));
                            const selectedPara = () =>
                                epubSelection() === para().text;
                            return (
                                <View
                                    selectable
                                    onPress={() => {
                                        epubSelection.set(para().text);
                                        epubSelectionScalarStart.set(
                                            para().scalarStart,
                                        );
                                    }}
                                    onSelectionChange={({ text, start }) => {
                                        if (!text) return;
                                        const offset = utf16ToScalar(
                                            para().text,
                                            start ?? 0,
                                        );
                                        epubSelection.set(text);
                                        epubSelectionScalarStart.set(
                                            para().scalarStart + offset,
                                        );
                                    }}
                                    style={{
                                        paddingVertical: 6,
                                        paddingHorizontal:
                                            highlighted() || selectedPara()
                                                ? 12
                                                : 2,
                                        borderLeftWidth:
                                            highlighted() || selectedPara()
                                                ? 3
                                                : 0,
                                        borderColor: ink("text-accent"),
                                        borderRadius: 6,
                                        backgroundColor: ink(
                                            selectedPara()
                                                ? "surface-selected"
                                                : highlighted()
                                                  ? "surface-inset"
                                                  : "transparent",
                                        ),
                                    }}
                                >
                                    <Text
                                        style={{
                                            fontSize: epubFontSize(),
                                            lineHeight: Math.round(
                                                epubFontSize() *
                                                    epubLineHeight(),
                                            ),
                                            color: ink("text-primary"),
                                            userSelect: "text",
                                        }}
                                    >
                                        {para().text}
                                    </Text>
                                </View>
                            );
                        }}
                    </Each>
                </View>
            </Scroll>
            <If when={!!epubSelection()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 12 : 20,
                        paddingVertical: 10,
                        flexDirection: "row",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: 8,
                        backgroundColor: ink("surface-floating"),
                        borderTopWidth: 1,
                        borderColor: ink("border-control"),
                    }}
                >
                    <Copy size={12} strong>
                        {`已选 ${Array.from(epubSelection()).length} 字`}
                    </Copy>
                    <View style={{ flex: 1 }} />
                    <Tool
                        label="复制当前选文"
                        text="复制"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void Clipboard.writeText(epubSelection())
                        }
                    />
                    <Tool
                        label="保存当前选文到 Core"
                        text="保存摘录"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void captureEpubExcerpt(book, epubChapterIndex())
                        }
                    />
                    <Tool
                        label="写想法并保存"
                        icon="pen"
                        text="写想法并保存"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() => epubNoteSheetOpen.set(true)}
                    />
                    <Tool
                        label="收词到词书"
                        icon="book"
                        text="收词到词书"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void collectSelectionToVocab(
                                epubSelection(),
                                epubChapterText(),
                                book.title,
                                (msg) => epubChapterStatus.set(msg),
                            )
                        }
                    />
                    <TextButton
                        label="取消"
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => {
                            epubSelection.set("");
                            epubSelectionNote.set("");
                        }}
                    />
                </View>
            </If>
            <BottomSheet
                open={epubNoteSheetOpen()}
                onClose={() => epubNoteSheetOpen.set(false)}
                title="写想法并保存到 Core"
                subtitle={`《${book.title}》 · ${epubChapterTitle()}`}
            >
                <View style={{ gap: 12 }}>
                    <View
                        style={{
                            padding: 10,
                            borderRadius: 8,
                            borderLeftWidth: 3,
                            borderColor: ink("text-accent"),
                            backgroundColor: ink("surface-inset"),
                        }}
                    >
                        <Copy size={12} lines={3}>
                            {epubSelection()}
                        </Copy>
                    </View>
                    <Input
                        accessibilityLabel="为 EPUB 摘录写下想法"
                        multiline
                        value={epubSelectionNote()}
                        onChange={({ value }) => epubSelectionNote.set(value)}
                        placeholder="写下自己的想法（可选）"
                        style={{
                            minHeight: 88,
                            padding: 12,
                            fontSize: 15,
                            lineHeight: 24,
                            color: ink("text-primary"),
                            backgroundColor: ink("surface-base"),
                            borderWidth: 1,
                            borderColor: ink("border-control"),
                            borderRadius: 8,
                        }}
                        focusStyle={focus()}
                    />
                    <View
                        style={{
                            flexDirection: "row",
                            justifyContent: "flex-end",
                            gap: 10,
                        }}
                    >
                        <TextButton
                            label="取消"
                            size="touch"
                            onPress={() => epubNoteSheetOpen.set(false)}
                        />
                        <Tool
                            label="保存 EPUB 摘录与想法"
                            text="保存到 Core"
                            raised
                            size="touch"
                            onPress={() =>
                                void captureEpubExcerpt(
                                    book,
                                    epubChapterIndex(),
                                )
                            }
                        />
                    </View>
                </View>
            </BottomSheet>
            <BottomSheet
                open={epubTocOpen()}
                onClose={() => epubTocOpen.set(false)}
                title="章节目录"
                subtitle={`《${book.title}》 · 共 ${chapters} 章`}
            >
                <Scroll style={{ maxHeight: 360 }}>
                    <View style={{ gap: 6 }}>
                        <Each
                            of={
                                epubTocEntries().length > 0
                                    ? epubTocEntries()
                                    : Array.from(
                                          { length: chapters },
                                          (_, index) => ({
                                              title: `第 ${index + 1} 章`,
                                              index,
                                              depth: 0,
                                          }),
                                      )
                            }
                            key={(item) =>
                                `epub-toc-${item.index}-${item.title}`
                            }
                        >
                            {(item) => (
                                <Button
                                    accessibilityLabel={`打开章节 ${item().title}`}
                                    onPress={() => {
                                        epubTocOpen.set(false);
                                        void loadEpubChapterContent(
                                            book,
                                            item().index,
                                        );
                                    }}
                                    style={{
                                        minHeight: props.compact ? 44 : 42,
                                        paddingLeft: 10 + item().depth * 14,
                                        paddingRight: 10,
                                        flexDirection: "row",
                                        alignItems: "center",
                                        borderRadius: 7,
                                        backgroundColor: ink(
                                            epubChapterIndex() === item().index
                                                ? "surface-selected"
                                                : "transparent",
                                        ),
                                    }}
                                    hoverStyle={{
                                        backgroundColor: ink("surface-hover"),
                                    }}
                                    focusStyle={focus()}
                                >
                                    <View style={{ flex: 1, minWidth: 0 }}>
                                        <Copy>{item().title}</Copy>
                                    </View>
                                    <Copy size={11} muted>
                                        {`第 ${item().index + 1} 章`}
                                    </Copy>
                                </Button>
                            )}
                        </Each>
                    </View>
                </Scroll>
            </BottomSheet>
            <BottomSheet
                open={epubTypographyOpen()}
                onClose={() => epubTypographyOpen.set(false)}
                title="阅读排版设置"
                subtitle="字号与行高自动保存在本机"
            >
                <View style={{ gap: 14 }}>
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 10,
                        }}
                    >
                        <Copy strong>字号</Copy>
                        <View style={{ flex: 1 }} />
                        <Tool
                            label="减小字号"
                            text="A−"
                            size={props.compact ? "touch" : "regular"}
                            raised
                            disabled={epubFontSize() <= MIN_EPUB_FONT_SIZE}
                            onPress={() =>
                                void saveEpubTypography(
                                    epubFontSize() - 1,
                                    epubLineHeight(),
                                )
                            }
                        />
                        <Copy strong>{`${epubFontSize()} px`}</Copy>
                        <Tool
                            label="增大字号"
                            text="A＋"
                            size={props.compact ? "touch" : "regular"}
                            raised
                            disabled={epubFontSize() >= MAX_EPUB_FONT_SIZE}
                            onPress={() =>
                                void saveEpubTypography(
                                    epubFontSize() + 1,
                                    epubLineHeight(),
                                )
                            }
                        />
                    </View>
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 10,
                        }}
                    >
                        <Copy strong>行高</Copy>
                        <View style={{ flex: 1 }} />
                        <Tool
                            label="减小行高"
                            text="紧凑"
                            size={props.compact ? "touch" : "regular"}
                            raised
                            disabled={
                                epubLineHeight() <= MIN_EPUB_LINE_HEIGHT
                            }
                            onPress={() =>
                                void saveEpubTypography(
                                    epubFontSize(),
                                    epubLineHeight() - 0.1,
                                )
                            }
                        />
                        <Copy strong>{`${epubLineHeight().toFixed(1)}x`}</Copy>
                        <Tool
                            label="增大行高"
                            text="舒展"
                            size={props.compact ? "touch" : "regular"}
                            raised
                            disabled={
                                epubLineHeight() >= MAX_EPUB_LINE_HEIGHT
                            }
                            onPress={() =>
                                void saveEpubTypography(
                                    epubFontSize(),
                                    epubLineHeight() + 0.1,
                                )
                            }
                        />
                    </View>
                </View>
            </BottomSheet>
        </View>
    );
}

export function LibraryWorkspace(props: { compact: boolean }) {
    const target = librarySourceTarget();
    if (!libraryBooted) {
        libraryBooted = true;
        void restoreLibraryZoom();
        void restoreEpubTypography();
        if (!target) void loadLibraryPage();
    }
    if (target && target.serial !== handledLibrarySourceTarget) {
        handledLibrarySourceTarget = target.serial;
        void openLibrarySourceTarget(target);
    }
    return (
        <View style={{ flex: 1 }}>
            <If when={!!libraryOpenDoc()}>
                <LibraryReader compact={props.compact} />
            </If>
            <If when={!libraryOpenDoc() && !!libraryOpenEpub()}>
                <EpubReader compact={props.compact} />
            </If>
            <If when={!libraryOpenDoc() && !libraryOpenEpub()}>
                <LibraryList compact={props.compact} />
            </If>
        </View>
    );
}

function LibraryList(props: { compact: boolean }) {
    return (
        <Scroll style={{ flex: 1 }}>
            <View
                accessibilityLabel="资料库插件工作区"
                style={{
                    padding: props.compact ? 18 : 40,
                    maxWidth: 900,
                    width: "100%",
                    alignSelf: "center",
                    gap: 18,
                }}
            >
                <View
                    style={{
                        flexDirection: "row",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: 10,
                    }}
                >
                    <Copy size={25} strong>
                        资料库
                    </Copy>
                    <View style={{ flex: 1 }} />
                    <Tool
                        label="导入本地 PDF"
                        icon="plus"
                        text={
                            libraryImporting() ? "正在导入…" : "导入 PDF"
                        }
                        raised
                        size={props.compact ? "touch" : "regular"}
                        disabled={libraryImporting()}
                        onPress={() => void importLibraryPdf()}
                    />
                    <Tool
                        label="导入本地 EPUB"
                        icon="book"
                        text={
                            libraryImporting() ? "正在导入…" : "导入 EPUB"
                        }
                        raised
                        size={props.compact ? "touch" : "regular"}
                        disabled={libraryImporting()}
                        onPress={() => void importLibraryEpub()}
                    />
                </View>
                <Copy size={12} muted>
                    {libraryStatus()}
                </Copy>
                <If
                    when={
                        libraryRows().length + libraryEpubRows().length > 0
                    }
                >
                    <Copy size={11} muted>
                        {`共 ${libraryRows().length} 份 PDF · ${libraryEpubRows().length} 本 EPUB`}
                    </Copy>
                </If>
                <Each
                    of={libraryRows()}
                    key={(row) => `pdf-doc-${row.document_id}`}
                >
                    {(row) => (
                        <View
                            style={{
                                paddingVertical: 12,
                                paddingHorizontal: 10,
                                borderBottomWidth: 1,
                                borderColor: ink("border-subtle"),
                                flexDirection: "row",
                                alignItems: "center",
                                gap: 12,
                                borderRadius: 8,
                            }}
                        >
                            <Button
                                accessibilityLabel={`${
                                    libraryPositionOf(row())
                                        ? "继续阅读"
                                        : "阅读"
                                }${row().title}`}
                                onPress={() =>
                                    void openLibraryDocument(row())
                                }
                                style={{
                                    flex: 1,
                                    minWidth: 0,
                                    minHeight: props.compact ? 44 : undefined,
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 12,
                                }}
                                hoverStyle={{
                                    backgroundColor: ink("surface-hover"),
                                }}
                                activeStyle={{
                                    backgroundColor: ink("surface-pressed"),
                                }}
                                focusStyle={focus()}
                            >
                                <Badge icon="document" size={38} />
                                <View style={{ flex: 1, gap: 4, minWidth: 0 }}>
                                    <View
                                        style={{
                                            flexDirection: "row",
                                            alignItems: "center",
                                            gap: 6,
                                        }}
                                    >
                                        <View
                                            style={{
                                                paddingHorizontal: 6,
                                                paddingVertical: 1,
                                                borderRadius: 4,
                                                backgroundColor:
                                                    ink("surface-inset"),
                                            }}
                                        >
                                            <Copy size={10} strong>
                                                PDF
                                            </Copy>
                                        </View>
                                        <View style={{ flex: 1, minWidth: 0 }}>
                                            <Copy size={15} strong>
                                                {row().title}
                                            </Copy>
                                        </View>
                                    </View>
                                    <Copy size={11} muted>
                                        {row().page_count} 页 ·{" "}
                                        {Math.max(
                                            1,
                                            Math.round(
                                                row().original_bytes / 1024,
                                            ),
                                        )}{" "}
                                        KiB · 提交 {row().commit_rev}
                                        {libraryPositionLabel(row())}
                                    </Copy>
                                </View>
                                <Copy size={12} muted>
                                    {libraryPositionOf(row())
                                        ? "继续"
                                        : "阅读"}
                                </Copy>
                            </Button>
                            <Tool
                                label={`删除 ${row().title}`}
                                icon="trash"
                                size={props.compact ? "touch" : "compact"}
                                onPress={() =>
                                    void deleteLibraryDocument(
                                        row().document_id,
                                        "pdf",
                                        row().title,
                                    )
                                }
                            />
                        </View>
                    )}
                </Each>
                <Each
                    of={libraryEpubRows()}
                    key={(row) => `epub-doc-${row.document_id}`}
                >
                    {(row) => (
                        <View
                            style={{
                                paddingVertical: 12,
                                paddingHorizontal: 10,
                                borderBottomWidth: 1,
                                borderColor: ink("border-subtle"),
                                flexDirection: "row",
                                alignItems: "center",
                                gap: 12,
                                borderRadius: 8,
                            }}
                        >
                            <Button
                                accessibilityLabel={`${
                                    epubPositionOf(row())
                                        ? "继续阅读"
                                        : "阅读"
                                }${row().title}`}
                                onPress={() => void openEpubDocument(row())}
                                style={{
                                    flex: 1,
                                    minWidth: 0,
                                    minHeight: props.compact ? 44 : undefined,
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 12,
                                }}
                                hoverStyle={{
                                    backgroundColor: ink("surface-hover"),
                                }}
                                activeStyle={{
                                    backgroundColor: ink("surface-pressed"),
                                }}
                                focusStyle={focus()}
                            >
                                <Badge
                                    icon="book"
                                    size={38}
                                    tone="surface-accent"
                                />
                                <View style={{ flex: 1, gap: 4, minWidth: 0 }}>
                                    <View
                                        style={{
                                            flexDirection: "row",
                                            alignItems: "center",
                                            gap: 6,
                                        }}
                                    >
                                        <View
                                            style={{
                                                paddingHorizontal: 6,
                                                paddingVertical: 1,
                                                borderRadius: 4,
                                                backgroundColor:
                                                    ink("surface-selected"),
                                            }}
                                        >
                                            <Copy size={10} strong>
                                                EPUB
                                            </Copy>
                                        </View>
                                        <View style={{ flex: 1, minWidth: 0 }}>
                                            <Copy size={15} strong>
                                                {row().title}
                                            </Copy>
                                        </View>
                                    </View>
                                    <Copy size={11} muted>
                                        {row().author
                                            ? `${row().author} · `
                                            : ""}
                                        {row().chapter_count} 章 ·{" "}
                                        {Math.max(
                                            1,
                                            Math.round(
                                                row().original_bytes / 1024,
                                            ),
                                        )}{" "}
                                        KiB
                                        {epubPositionLabel(row())}
                                    </Copy>
                                </View>
                                <Copy size={12} muted>
                                    {epubPositionOf(row()) ? "继续" : "阅读"}
                                </Copy>
                            </Button>
                            <Tool
                                label={`删除 ${row().title}`}
                                icon="trash"
                                size={props.compact ? "touch" : "compact"}
                                onPress={() =>
                                    void deleteLibraryDocument(
                                        row().document_id,
                                        "epub",
                                        row().title,
                                    )
                                }
                            />
                        </View>
                    )}
                </Each>
                <If
                    when={
                        !libraryRows().length &&
                        !libraryEpubRows().length &&
                        libraryStatus().includes("没有")
                    }
                >
                    <Copy muted>
                        导入 PDF 或 EPUB 后，资料会出现在这里；当前不会上传原文件。
                    </Copy>
                </If>
                <If when={!!libraryNext()}>
                    <Tool
                        label="加载更多资料"
                        text="加载更多"
                        size={props.compact ? "touch" : "regular"}
                        onPress={() => void loadLibraryPage(libraryNext())}
                    />
                </If>
            </View>
        </Scroll>
    );
}
