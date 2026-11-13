import { expect, test } from "nsh/test";
import {
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
