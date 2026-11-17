import { expect, test } from "nsh/test";
import {
    displayBoxes,
    fitPageSize,
    parseDocumentHandle,
    pdfHighlightBoxes,
    resolveQuoteRange,
} from "../document_surface";

const geometry = {
    width: 200,
    height: 100,
    glyphs: [
        [0, 10, 10, 10, 10],
        [1, 20, 10, 10, 10],
        [2, 30, 10, 10, 10],
    ],
};

test("parseDocumentHandle accepts section and focus handles", () => {
    expect(parseDocumentHandle("reader-doc:pdf:7:0")).toEqual({
        documentId: 7,
        format: "pdf",
        sectionIndex: 0,
        mode: "section",
    });
    expect(parseDocumentHandle("reader-doc:focus-12:pdf:7:3")).toEqual({
        documentId: 7,
        format: "pdf",
        sectionIndex: 3,
        mode: "focus",
        captureId: 12,
    });
});

test("parseDocumentHandle strictly rejects malformed handles", () => {
    const invalid = [
        "reader-doc:7:0",
        "reader-doc:pdf:7",
        "reader-doc:pdf:7:0:0",
        "reader-doc:pdf:07:0",
        "reader-doc:pdf:7:00",
        "reader-doc:pdf:0:0",
        "reader-doc:pdf:-1:0",
        "reader-doc:pdf:7:-1",
        "reader-doc:pdf:1.5:0",
        "reader-doc:focus-0:pdf:7:0",
        "reader-doc:focus-12:pdf:7",
        "reader-doc:focus-12:xlsx:7:0",
        "reader-doc:pdf:9007199254740992:0",
        "reader-doc:pdf:7:9007199254740992",
    ];
    for (const raw of invalid) {
        expect(parseDocumentHandle(raw)).toBeNull();
    }
});

test("fitPageSize scales down while preserving ratio", () => {
    expect(fitPageSize(1000, 2000, false)).toEqual({ width: 320, height: 640 });
    expect(fitPageSize(1000, 2000, true)).toEqual({ width: 190, height: 380 });
    expect(fitPageSize(100, 200, false)).toEqual({ width: 100, height: 200 });
    expect(fitPageSize(0, 200, false)).toEqual({ width: 0, height: 0 });
    expect(fitPageSize(1000, Number.POSITIVE_INFINITY, true)).toEqual({ width: 0, height: 0 });
});

test("displayBoxes scales boxes from native geometry", () => {
    expect(
        displayBoxes(
            [
                [10, 20, 30, 40],
                [0, 0, 200, 100],
            ],
            { width: 200, height: 100 },
            { width: 100, height: 50 },
        ),
    ).toEqual([
        [5, 10, 15, 20],
        [0, 0, 100, 50],
    ]);
    expect(
        displayBoxes(
            [
                [10, 20, 30, 40],
                [-1, 0, 2, 2],
                [0, 0, 0, 1],
            ],
            { width: 0, height: 100 },
            { width: 100, height: 50 },
        ),
    ).toEqual([]);
});

test("resolveQuoteRange finds a unique quote and rejects ambiguity", () => {
    expect(resolveQuoteRange("alpha beta gamma", "beta gamma")).toEqual({
        ok: true,
        start: 6,
        end: 16,
    });
    expect(resolveQuoteRange("alpha beta gamma", "missing")).toMatchObject({
        ok: false,
    });
    expect(resolveQuoteRange("repeat repeat", "repeat")).toMatchObject({
        ok: false,
    });
    expect(resolveQuoteRange("alpha beta", "")).toEqual({
        ok: true,
        start: 0,
        end: 10,
    });
    expect(resolveQuoteRange("alpha beta", "alpha")).toEqual({
        ok: true,
        start: 0,
        end: 5,
    });
});

test("resolveQuoteRange counts Unicode scalars and caps empty quotes at 64", () => {
    const emojiText = "😀-😀";
    expect(resolveQuoteRange(emojiText, "-😀")).toEqual({ ok: true, start: 1, end: 3 });
    expect(resolveQuoteRange("", "")).toMatchObject({ ok: false });
    const short = "a".repeat(63);
    expect(resolveQuoteRange(short, "")).toEqual({ ok: true, start: 0, end: 63 });
    const long = "a".repeat(64);
    expect(resolveQuoteRange(long, "")).toEqual({ ok: true, start: 0, end: 64 });
    const longer = "a".repeat(65);
    expect(resolveQuoteRange(longer, "")).toEqual({ ok: true, start: 0, end: 64 });
});

test("pdfHighlightBoxes selects saved ranges or only the focus capture", () => {
    const ranges = [
        { capture_id: 8, start: 0, end: 1 },
        { capture_id: 9, start: 1, end: 2 },
    ];
    const all = pdfHighlightBoxes(JSON.stringify(geometry), false, ranges, null);
    expect(all.ok).toBe(true);
    expect(all.ok && all.boxes.length).toBe(2);

    const focused = pdfHighlightBoxes(JSON.stringify(geometry), false, ranges, 9);
    expect(focused.ok).toBe(true);
    expect(focused.ok && focused.boxes.length).toBe(1);

    const missing = pdfHighlightBoxes(JSON.stringify(geometry), false, ranges, 99);
    expect(missing.ok).toBe(false);
});

test("pdfHighlightBoxes reports malformed geometry and compact display", () => {
    const malformed = pdfHighlightBoxes(
        "{bad",
        false,
        [{ capture_id: 1, start: 0, end: 1 }],
        null,
    );
    expect(malformed.ok).toBe(false);

    const compact = pdfHighlightBoxes(
        JSON.stringify(geometry),
        true,
        [{ capture_id: 1, start: 0, end: 1 }],
        null,
    );
    expect(compact.ok).toBe(true);
    expect(compact.ok && compact.display.width).toBeLessThanOrEqual(280);
    expect(compact.ok && compact.boxes[0][0]).toBeLessThanOrEqual(compact.display.width);
});
