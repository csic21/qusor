import { expect, test } from "nsh/test";
import {
    boxesForSavedRanges,
    boxesForScalarRange,
    glyphAt,
    parsePageGeometry,
    selectionBetween,
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
