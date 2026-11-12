import { Storage } from "nsh";

const PREFIX = "qusor.library.position.v1.";
const MAX_PAGES = 100_000;

export type ReadingPosition = {
    page: number;
    pageCount: number;
    updatedAt: number;
};

export function readingPositionKey(documentId: number): string {
    if (!Number.isSafeInteger(documentId) || documentId <= 0)
        throw Error("invalid document id");
    return `${PREFIX}${documentId}`;
}

export function parseReadingPosition(
    raw: string | null,
    pageCount: number,
): ReadingPosition | null {
    if (!raw || !Number.isSafeInteger(pageCount)) return null;
    if (pageCount <= 0 || pageCount > MAX_PAGES) return null;
    try {
        const value = JSON.parse(raw);
        if (
            !Number.isSafeInteger(value?.page) ||
            !Number.isSafeInteger(value?.pageCount) ||
            !Number.isSafeInteger(value?.updatedAt) ||
            value.page < 0 ||
            value.page >= value.pageCount ||
            value.pageCount !== pageCount ||
            value.updatedAt <= 0
        ) {
            return null;
        }
        return {
            page: value.page,
            pageCount: value.pageCount,
            updatedAt: value.updatedAt,
        };
    } catch (_) {
        return null;
    }
}

export async function loadReadingPosition(
    documentId: number,
    pageCount: number,
): Promise<ReadingPosition | null> {
    try {
        return parseReadingPosition(
            await Storage.getItem(readingPositionKey(documentId)),
            pageCount,
        );
    } catch (_) {
        return null;
    }
}

export async function saveReadingPosition(
    documentId: number,
    page: number,
    pageCount: number,
): Promise<ReadingPosition | null> {
    if (
        !Number.isSafeInteger(page) ||
        !Number.isSafeInteger(pageCount) ||
        page < 0 ||
        page >= pageCount ||
        pageCount > MAX_PAGES
    ) {
        return null;
    }
    const position = { page, pageCount, updatedAt: Date.now() };
    try {
        await Storage.setItem(
            readingPositionKey(documentId),
            JSON.stringify(position),
        );
        return position;
    } catch (_) {
        return null;
    }
}
