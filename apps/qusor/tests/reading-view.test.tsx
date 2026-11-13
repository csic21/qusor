import { expect, open, test } from "nsh/test";
import {
    DEFAULT_EPUB_FONT_SIZE,
    DEFAULT_EPUB_LINE_HEIGHT,
    DEFAULT_READING_ZOOM,
    formatEpubPosition,
    normalizeEpubFontSize,
    normalizeEpubLineHeight,
    normalizeReadingZoom,
    nextReadingZoom,
    parseEpubPosition,
    readingZoomLabel,
} from "../reading_view";

test("page zoom is limited to the supported reading levels", () => {
    expect(normalizeReadingZoom(1.26)).toBe(1.25);
    expect(normalizeReadingZoom(1.74)).toBe(1.5);
    expect(normalizeReadingZoom(9)).toBe(2);
    expect(normalizeReadingZoom(-1)).toBe(DEFAULT_READING_ZOOM);
    expect(normalizeReadingZoom(Number.NaN)).toBe(DEFAULT_READING_ZOOM);
});

test("zoom controls stop at fit width and the maximum", () => {
    expect(nextReadingZoom(1, -1)).toBe(1);
    expect(nextReadingZoom(1, 1)).toBe(1.25);
    expect(nextReadingZoom(1.25, 1)).toBe(1.5);
    expect(nextReadingZoom(2, 1)).toBe(2);
    expect(nextReadingZoom(2, -1)).toBe(1.5);
});

test("zoom labels use the normalized percentage", () => {
    expect(readingZoomLabel(1)).toBe("100%");
    expect(readingZoomLabel(1.26)).toBe("125%");
    expect(readingZoomLabel(2)).toBe("200%");
});

test("the zoom preference reopens from app storage", async () => {
    const app = await open("./reading-zoom-harness.tsx", {
        storage: {
            "qusor.library.page-zoom.v1": JSON.stringify({ zoom: 1.5 }),
        },
    });
    await app.waitFor("text:ready");
    await app.waitFor("text:zoom 1.5");
});

test("moving the zoom control writes the app preference", async () => {
    const app = await open("./reading-zoom-harness.tsx");
    await app.waitFor("text:ready");
    await app.press("label:larger page");
    await app.waitFor("text:saved");
    expect(
        JSON.parse(app.storage()["qusor.library.page-zoom.v1"]).zoom,
    ).toBe(1.25);
});

test("epub font size and line height normalize into supported bounds", () => {
    expect(normalizeEpubFontSize(undefined)).toBe(DEFAULT_EPUB_FONT_SIZE);
    expect(normalizeEpubFontSize(Number.NaN)).toBe(DEFAULT_EPUB_FONT_SIZE);
    expect(normalizeEpubFontSize(10)).toBe(14);
    expect(normalizeEpubFontSize(19.4)).toBe(19);
    expect(normalizeEpubFontSize(32)).toBe(26);

    expect(normalizeEpubLineHeight(undefined)).toBe(DEFAULT_EPUB_LINE_HEIGHT);
    expect(normalizeEpubLineHeight(Number.POSITIVE_INFINITY)).toBe(
        DEFAULT_EPUB_LINE_HEIGHT,
    );
    expect(normalizeEpubLineHeight(1.1)).toBe(1.4);
    expect(normalizeEpubLineHeight(1.4 + 0.1)).toBe(1.5);
    expect(normalizeEpubLineHeight(2.6)).toBe(2.0);
});

test("epub positions format and parse with chapter bounds validation", () => {
    const encoded = formatEpubPosition({
        chapterIndex: 3,
        scrollRatio: 0.45678,
    });
    expect(parseEpubPosition(encoded)).toEqual({
        chapterIndex: 3,
        scrollRatio: 0.4568,
    });
    expect(parseEpubPosition(encoded, 5)).toEqual({
        chapterIndex: 3,
        scrollRatio: 0.4568,
    });
    expect(parseEpubPosition(encoded, 3)).toBeNull();
    expect(parseEpubPosition("", 5)).toBeNull();
    expect(parseEpubPosition("{bad", 5)).toBeNull();
    expect(
        parseEpubPosition(
            JSON.stringify({ chapterIndex: -1, scrollRatio: 0.5 }),
        ),
    ).toBeNull();
    expect(
        parseEpubPosition(
            JSON.stringify({ chapterIndex: 0, scrollRatio: 1.2 }),
        ),
    ).toBeNull();
});

test("epub positions clamp out-of-range values on format and reject invalid bounds on parse", () => {
    expect(
        parseEpubPosition(
            formatEpubPosition({ chapterIndex: -3, scrollRatio: -0.5 }),
        ),
    ).toEqual({ chapterIndex: 0, scrollRatio: 0 });
    expect(
        parseEpubPosition(
            formatEpubPosition({
                chapterIndex: 2.9,
                scrollRatio: 1.5,
            }),
            3,
        ),
    ).toEqual({ chapterIndex: 2, scrollRatio: 1 });
    expect(
        parseEpubPosition(
            formatEpubPosition({
                chapterIndex: Number.NaN,
                scrollRatio: Number.POSITIVE_INFINITY,
            }),
        ),
    ).toEqual({ chapterIndex: 0, scrollRatio: 0 });
    expect(parseEpubPosition(null, 5)).toBeNull();
    expect(parseEpubPosition(undefined, 5)).toBeNull();
    expect(
        parseEpubPosition(
            JSON.stringify({ chapterIndex: 0, scrollRatio: 0 }),
            0,
        ),
    ).toBeNull();
    expect(
        parseEpubPosition(
            JSON.stringify({ chapterIndex: 1.5, scrollRatio: 0.2 }),
            5,
        ),
    ).toBeNull();
});
