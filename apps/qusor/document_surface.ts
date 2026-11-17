import {
    boxesForSavedRanges,
    boxesForScalarRange,
    parsePageGeometry,
    type PageBox,
    type SavedSelectionRange,
} from "./reading_selection";

export type DocumentFormat = "pdf" | "epub";

export type ParsedDocumentHandle =
    | {
          mode: "section";
          format: DocumentFormat;
          documentId: number;
          sectionIndex: number;
      }
    | {
          mode: "focus";
          format: DocumentFormat;
          documentId: number;
          sectionIndex: number;
          captureId: number;
      };

export type Size = { width: number; height: number };

export type HighlightBox = [x: number, y: number, width: number, height: number];

export type QuoteRangeResult =
    | { ok: true; start: number; end: number }
    | { ok: false; error: string };

const FOCUS_PREFIX = "reader-doc:focus-";
const SECTION_PREFIX = "reader-doc:";
const MAX_DISPLAY_SIZE = { width: 480, height: 640 } as const;
const COMPACT_MAX_DISPLAY_SIZE = { width: 280, height: 380 } as const;
const EMPTY_QUOTE_SCALARS = 64;

function positiveSafeInteger(raw: string): number | null {
    if (!/^[1-9][0-9]*$/.test(raw)) return null;
    const value = Number(raw);
    return Number.isSafeInteger(value) && value > 0 ? value : null;
}

function safeSectionIndex(raw: string): number | null {
    if (!/^(0|[1-9][0-9]*)$/.test(raw)) return null;
    const value = Number(raw);
    return Number.isSafeInteger(value) ? value : null;
}

export function parseDocumentHandle(surfaceId: string): ParsedDocumentHandle | null {
    const value = surfaceId.trim();
    if (value.startsWith(FOCUS_PREFIX)) {
        const parts = value.slice(FOCUS_PREFIX.length).split(":");
        if (parts.length !== 4) return null;
        const [captureRaw, format, documentRaw, sectionRaw] = parts;
        if (format !== "pdf" && format !== "epub") return null;
        const documentId = positiveSafeInteger(documentRaw);
        const sectionIndex = safeSectionIndex(sectionRaw);
        const captureId = positiveSafeInteger(captureRaw);
        if (documentId === null || sectionIndex === null || captureId === null) {
            return null;
        }
        return {
            mode: "focus",
            format,
            documentId,
            sectionIndex,
            captureId,
        };
    }
    if (!value.startsWith(SECTION_PREFIX)) return null;
    const parts = value.slice(SECTION_PREFIX.length).split(":");
    if (parts.length !== 3) return null;
    const [format, documentRaw, sectionRaw] = parts;
    if (format !== "pdf" && format !== "epub") return null;
    const documentId = positiveSafeInteger(documentRaw);
    const sectionIndex = safeSectionIndex(sectionRaw);
    if (documentId === null || sectionIndex === null) return null;
    return {
        mode: "section",
        format,
        documentId,
        sectionIndex,
    };
}

export function fitPageSize(
    width: number,
    height: number,
    compact: boolean,
): Size {
    const maximum = compact ? COMPACT_MAX_DISPLAY_SIZE : MAX_DISPLAY_SIZE;
    if (
        !Number.isFinite(width) ||
        !Number.isFinite(height) ||
        width <= 0 ||
        height <= 0
    ) {
        return { width: 0, height: 0 };
    }
    const scale = Math.min(
        maximum.width / width,
        maximum.height / height,
        1,
    );
    return {
        width: width * scale,
        height: height * scale,
    };
}

export function displayBoxes(
    boxes: readonly PageBox[],
    geometry: Size,
    display: Size,
): HighlightBox[] {
    if (
        geometry.width <= 0 ||
        geometry.height <= 0 ||
        display.width <= 0 ||
        display.height <= 0
    ) {
        return [];
    }
    const scaleX = display.width / geometry.width;
    const scaleY = display.height / geometry.height;
    return boxes.map(([x, y, width, height]) => [
        x * scaleX,
        y * scaleY,
        width * scaleX,
        height * scaleY,
    ]);
}

export function resolveQuoteRange(
    text: string,
    quote: string,
): QuoteRangeResult {
    const scalars = Array.from(text);
    const target = quote.trim();
    if (!target) {
        if (scalars.length === 0) {
            return { ok: false, error: "当前章节无可摘录文本。" };
        }
        return {
            ok: true,
            start: 0,
            end: Math.min(scalars.length, EMPTY_QUOTE_SCALARS),
        };
    }
    const targetScalars = Array.from(target);
    const matches: number[] = [];
    for (
        let index = 0;
        index + targetScalars.length <= scalars.length;
        index += 1
    ) {
        let matched = true;
        for (let offset = 0; offset < targetScalars.length; offset += 1) {
            if (scalars[index + offset] !== targetScalars[offset]) {
                matched = false;
                break;
            }
        }
        if (matched) matches.push(index);
    }
    if (matches.length === 0) {
        return {
            ok: false,
            error: "未在当前章节找到这段原文；请核对文字或打开对应页。",
        };
    }
    if (matches.length > 1) {
        return {
            ok: false,
            error:
                "这段原文在当前章节出现多次，无法唯一确定位置；请摘录得更具体一些。",
        };
    }
    return {
        ok: true,
        start: matches[0],
        end: matches[0] + targetScalars.length,
    };
}

export type PdfHighlightResult =
    | { ok: true; boxes: PageBox[]; display: Size }
    | { ok: false; message: string };

export function pdfHighlightBoxes(
    rawGeometry: string,
    compact: boolean,
    ranges: readonly SavedSelectionRange[],
    focusCaptureId: number | null,
): PdfHighlightResult {
    if (!rawGeometry || rawGeometry.length > 512 * 1024) {
        return {
            ok: false,
            message:
                "已定位到该页及摘录，但该页旋转、映射失败或无字符几何，无法绘制精确高亮。",
        };
    }
    let declared: { width: number; height: number } | null = null;
    try {
        const parsed = JSON.parse(rawGeometry);
        if (
            typeof parsed?.width === "number" &&
            Number.isFinite(parsed.width) &&
            typeof parsed?.height === "number" &&
            Number.isFinite(parsed.height)
        ) {
            declared = { width: parsed.width, height: parsed.height };
        }
    } catch (_) {
        declared = null;
    }
    if (!declared || declared.width <= 0 || declared.height <= 0) {
        return {
            ok: false,
            message:
                "已定位到该页及摘录，但该页旋转、映射失败或无字符几何，无法绘制精确高亮。",
        };
    }
    const geometry = parsePageGeometry(
        rawGeometry,
        declared.width,
        declared.height,
    );
    if (!geometry) {
        return {
            ok: false,
            message:
                "已定位到该页及摘录，但该页旋转、映射失败或无字符几何，无法绘制精确高亮。",
        };
    }
    const focusRange =
        focusCaptureId === null
            ? null
            : ranges.find((range) => range.capture_id === focusCaptureId);
    if (focusCaptureId !== null && !focusRange) {
        return { ok: false, message: `当前页未找到摘录 #${focusCaptureId} 的字符区间。` };
    }
    const boxes = focusRange
        ? boxesForScalarRange(geometry, focusRange.start, focusRange.end)
        : boxesForSavedRanges(geometry, ranges);
    if (focusCaptureId !== null && boxes.length === 0) {
        return {
            ok: false,
            message: `摘录 #${focusCaptureId} 的字符区间没有对应的页面几何，无法绘制高亮。`,
        };
    }
    return {
        ok: true,
        boxes,
        display: fitPageSize(geometry.width, geometry.height, compact),
    };
}
