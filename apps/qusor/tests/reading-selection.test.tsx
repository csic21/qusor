import { expect, test } from "nsh/test";
import {
    adjustSelectionEnd,
    adjustSelectionStart,
    boxesForRange,
    boxesForSavedRanges,
    boxesForScalarRange,
    glyphAt,
    parsePageGeometry,
    selectionBetween,
    selectionHandles,
    type PageGeometry,
} from "../reading_selection";

const geometry: PageGeometry = {
    width: 200,
    height: 100,
    glyphs: [
        [0, 10, 10, 10, 10],
        [1, 20, 10, 10, 10],
        [2, 30, 10, 10, 10],
    ],
};

test("geometry is accepted only for the rendered page size", () => {
    const raw = JSON.stringify(geometry);
    expect(parsePageGeometry(raw, 200, 100)).toEqual(geometry);
    expect(parsePageGeometry(raw, 201, 100)).toBeNull();
    expect(parsePageGeometry("", 200, 100)).toBeNull();
    expect(
        parsePageGeometry(
            JSON.stringify({
                ...geometry,
                glyphs: [[2, 10, 10, 10, 10], [1, 20, 10, 10, 10]],
            }),
            200,
            100,
        ),
    ).toBeNull();
});

test("display points map to the smallest containing glyph", () => {
    expect(glyphAt(geometry, { width: 400, height: 200 }, 40, 40)).toBe(0);
    expect(glyphAt(geometry, { width: 400, height: 200 }, 60, 40)).toBe(1);
    expect(glyphAt(geometry, { width: 400, height: 200 }, 2, 2)).toBeNull();
});

test("glyph order produces the exact scalar range and page boxes", () => {
    expect(selectionBetween(geometry, "abc def", 2, 0)).toEqual({
        text: "abc",
        start: 0,
        boxes: [
            [10, 10, 10, 10],
            [20, 10, 10, 10],
            [30, 10, 10, 10],
        ],
    });
    expect(selectionBetween(geometry, "abc def", 0, 2)?.text).toBe("abc");
    expect(selectionBetween(geometry, "ab", 0, 2)).toBeNull();
});

test("saved scalar ranges map to the same persisted glyph boxes", () => {
    expect(boxesForScalarRange(geometry, 1, 3)).toEqual([
        [20, 10, 10, 10],
        [30, 10, 10, 10],
    ]);
    expect(boxesForScalarRange(geometry, 3, 3)).toEqual([]);
    expect(boxesForScalarRange(geometry, 0, 3)).toHaveLength(3);
});

test("overlapping saved ranges share one visual highlight", () => {
    expect(
        boxesForSavedRanges(geometry, [
            { capture_id: 2, start: 1, end: 3 },
            { capture_id: 1, start: 0, end: 2 },
            { capture_id: 3, start: 3, end: 3 },
        ]),
    ).toEqual([
        [10, 10, 10, 10],
        [20, 10, 10, 10],
        [30, 10, 10, 10],
    ]);
    expect(
        boxesForSavedRanges(geometry, [
            { capture_id: 1, start: 2, end: 3 },
        ]),
    ).toEqual([[30, 10, 10, 10]]);
});

const multiLineGeometry: PageGeometry = {
    width: 200,
    height: 100,
    glyphs: [
        [0, 10, 10, 10, 10],
        [1, 20, 10, 10, 10],
        [2, 30, 10, 10, 10],
        [4, 10, 30, 10, 10],
        [5, 20, 30, 10, 10],
        [6, 30, 30, 10, 10],
    ],
};

test("boxesForRange and selectionHandles expose start and end drag boxes", () => {
    expect(boxesForRange(multiLineGeometry, 1, 6)).toEqual([
        [20, 10, 10, 10],
        [30, 10, 10, 10],
        [10, 30, 10, 10],
        [20, 30, 10, 10],
    ]);
    expect(selectionHandles(multiLineGeometry, 1, 6)).toEqual({
        startBox: [20, 10, 10, 10],
        endBox: [20, 30, 10, 10],
    });
    expect(selectionHandles(multiLineGeometry, 3, 3)).toBeNull();
    expect(selectionHandles(multiLineGeometry, 5, 2)).toBeNull();
});

test("adjustSelectionStart and adjustSelectionEnd resize across lines and clamp ordered bounds", () => {
    const display = { width: 400, height: 200 };
    // Expand start backwards to glyph 0 on line 1 (x=25..30 -> 12.5..15 in page coords)
    expect(
        adjustSelectionStart(multiLineGeometry, display, 30, 30, 1, 6),
    ).toEqual({ start: 0, end: 6 });

    // Drag start past currentEnd (6) onto glyph 5 (scalar 6): clamps to last glyph before 6 (scalar 5)
    expect(
        adjustSelectionStart(multiLineGeometry, display, 70, 70, 1, 6),
    ).toEqual({ start: 5, end: 6 });

    // Expand end forward onto glyph 5 (scalar 6 -> exclusive end 7)
    expect(
        adjustSelectionEnd(multiLineGeometry, display, 70, 70, 1, 5),
    ).toEqual({ start: 1, end: 7 });

    // Drag end before currentStart (4) onto glyph 0 (scalar 0): clamps to at least first glyph >= 4 (scalar 4 -> end 5)
    expect(
        adjustSelectionEnd(multiLineGeometry, display, 30, 30, 4, 7),
    ).toEqual({ start: 4, end: 5 });

    // Snap cleanly from horizontal whitespace on the same line
    expect(
        adjustSelectionEnd(multiLineGeometry, display, 160, 70, 0, 5),
    ).toEqual({ start: 0, end: 7 });
});
