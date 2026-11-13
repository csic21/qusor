import { Storage } from "nsh";

const KEY = "qusor.library.page-zoom.v1";
export const READING_ZOOM_LEVELS = [1, 1.25, 1.5, 2] as const;
export const DEFAULT_READING_ZOOM = READING_ZOOM_LEVELS[0];

export function normalizeReadingZoom(value: unknown): number {
    if (typeof value !== "number" || !Number.isFinite(value)) {
        return DEFAULT_READING_ZOOM;
    }
    let nearest: number = READING_ZOOM_LEVELS[0];
    for (const level of READING_ZOOM_LEVELS) {
        if (Math.abs(level - value) < Math.abs(nearest - value)) {
            nearest = level;
        }
    }
    return nearest;
}

export function nextReadingZoom(
    current: number,
    direction: -1 | 1,
): number {
    const normalized = normalizeReadingZoom(current);
    if (direction > 0) {
        return (
            READING_ZOOM_LEVELS.find((level) => level > normalized) ??
            READING_ZOOM_LEVELS[READING_ZOOM_LEVELS.length - 1]
        );
    }
    let lower: number = READING_ZOOM_LEVELS[0];
    for (const level of READING_ZOOM_LEVELS) {
        if (level < normalized) lower = level;
    }
    return lower;
}

export function readingZoomLabel(zoom: number): string {
    return `${Math.round(normalizeReadingZoom(zoom) * 100)}%`;
}

export async function loadReadingZoom(): Promise<number> {
    try {
        const raw = await Storage.getItem(KEY);
        if (!raw) return DEFAULT_READING_ZOOM;
        return normalizeReadingZoom(JSON.parse(raw)?.zoom);
    } catch (_) {
        return DEFAULT_READING_ZOOM;
    }
}

export async function saveReadingZoom(zoom: number): Promise<number> {
    const normalized = normalizeReadingZoom(zoom);
    try {
        await Storage.setItem(KEY, JSON.stringify({ zoom: normalized }));
        return normalized;
    } catch (_) {
        return normalized;
    }
}

export const MIN_EPUB_FONT_SIZE = 14;
export const MAX_EPUB_FONT_SIZE = 26;
export const DEFAULT_EPUB_FONT_SIZE = 17;

export const MIN_EPUB_LINE_HEIGHT = 1.4;
export const MAX_EPUB_LINE_HEIGHT = 2.0;
export const DEFAULT_EPUB_LINE_HEIGHT = 1.7;

export type EpubPosition = {
    chapterIndex: number;
    scrollRatio: number;
};

/** Normalizes an EPUB reader font size into `[14, 26]`, defaulting to `17`. */
export function normalizeEpubFontSize(size: unknown): number {
    if (typeof size !== "number" || !Number.isFinite(size)) {
        return DEFAULT_EPUB_FONT_SIZE;
    }
    return Math.round(
        Math.min(MAX_EPUB_FONT_SIZE, Math.max(MIN_EPUB_FONT_SIZE, size)),
    );
}

/** Normalizes an EPUB reader line-height ratio into `[1.4, 2.0]`, defaulting to `1.7`. */
export function normalizeEpubLineHeight(ratio: unknown): number {
    if (typeof ratio !== "number" || !Number.isFinite(ratio)) {
        return DEFAULT_EPUB_LINE_HEIGHT;
    }
    const clamped = Math.min(
        MAX_EPUB_LINE_HEIGHT,
        Math.max(MIN_EPUB_LINE_HEIGHT, ratio),
    );
    return Math.round(clamped * 10) / 10;
}

/** Serializes an EPUB chapter index and normalized scroll ratio (`[0, 1]`). */
export function formatEpubPosition(position: EpubPosition): string {
    const chapterIndex =
        typeof position.chapterIndex === "number" &&
        Number.isFinite(position.chapterIndex)
            ? Math.max(0, Math.floor(position.chapterIndex))
            : 0;
    const rawRatio =
        typeof position.scrollRatio === "number" &&
        Number.isFinite(position.scrollRatio)
            ? position.scrollRatio
            : 0;
    const scrollRatio =
        Math.round(Math.min(1, Math.max(0, rawRatio)) * 10_000) / 10_000;
    return JSON.stringify({ chapterIndex, scrollRatio });
}

/** Parses a serialized EPUB position, validating bounds when `totalChapters` is provided. */
export function parseEpubPosition(
    raw: string | null | undefined,
    totalChapters?: number,
): EpubPosition | null {
    if (!raw || typeof raw !== "string") return null;
    try {
        const value = JSON.parse(raw);
        if (!value || typeof value !== "object") return null;
        const { chapterIndex, scrollRatio } = value as {
            chapterIndex?: unknown;
            scrollRatio?: unknown;
        };
        if (
            typeof chapterIndex !== "number" ||
            !Number.isSafeInteger(chapterIndex) ||
            chapterIndex < 0
        ) {
            return null;
        }
        if (typeof totalChapters === "number") {
            if (
                !Number.isSafeInteger(totalChapters) ||
                totalChapters <= 0 ||
                chapterIndex >= totalChapters
            ) {
                return null;
            }
        }
        if (
            typeof scrollRatio !== "number" ||
            !Number.isFinite(scrollRatio) ||
            scrollRatio < 0 ||
            scrollRatio > 1
        ) {
            return null;
        }
        return {
            chapterIndex,
            scrollRatio: Math.round(scrollRatio * 10_000) / 10_000,
        };
    } catch (_) {
        return null;
    }
}
