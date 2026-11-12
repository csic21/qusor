import { expect, open, test } from "nsh/test";
import {
    parseReadingPosition,
    readingPositionKey,
} from "../reading_position";

test("a stored position is accepted only for the same page count", () => {
    const raw = JSON.stringify({
        page: 2,
        pageCount: 4,
        updatedAt: 1_760_000_000_000,
    });
    expect(parseReadingPosition(raw, 4)).toEqual({
        page: 2,
        pageCount: 4,
        updatedAt: 1_760_000_000_000,
    });
    expect(parseReadingPosition(raw, 5)).toBeNull();
});

test("malformed or out-of-range positions are ignored", () => {
    for (const raw of [
        null,
        "",
        "{}",
        '{"page":4,"pageCount":4,"updatedAt":1}',
        '{"page":-1,"pageCount":4,"updatedAt":1}',
        '{"page":1.5,"pageCount":4,"updatedAt":1}',
        '{"page":1,"pageCount":0,"updatedAt":1}',
        '{"page":1,"pageCount":4,"updatedAt":0}',
        "not json",
    ]) {
        expect(parseReadingPosition(raw, 4)).toBeNull();
    }
});

test("position keys are scoped by a positive document id", () => {
    expect(readingPositionKey(7)).toBe("qusor.library.position.v1.7");
    expect(() => readingPositionKey(0)).toThrow();
    expect(() => readingPositionKey(1.5)).toThrow();
});

test("a reopened app resumes the stored page", async () => {
    const app = await open("./reading-position-harness.tsx", {
        storage: {
            "qusor.library.position.v1.7": JSON.stringify({
                page: 2,
                pageCount: 4,
                updatedAt: 1_760_000_000_000,
            }),
        },
    });
    await app.waitFor("text:resumed 2");
});

test("moving the page writes the app preference", async () => {
    const app = await open("./reading-position-harness.tsx");
    await app.waitFor("text:new");
    await app.press("label:next page");
    await app.waitFor("text:saved 1");
    await app.press("label:next page");
    await app.waitFor("text:saved 2");
    const stored = JSON.parse(
        app.storage()["qusor.library.position.v1.7"],
    );
    expect(stored.page).toBe(2);
    expect(stored.pageCount).toBe(4);
});
