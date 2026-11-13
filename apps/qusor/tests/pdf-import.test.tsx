import { expect, open, test } from "nsh/test";

test("a real PDF enters Core and renders its first page", async () => {
    const app = await open("./pdf-import-harness.tsx");
    await app.press("label:import pdf");
    await app.waitFor("text:imported", { timeout: 30000 });
    const details = JSON.parse(app.texts().find((text) => text.startsWith("{"))!);
    expect(details.page_count).toBe(2);
    expect(details.image_ok).toBe(true);
    expect(details.image_width).toBeGreaterThan(0);
    expect(details.file_bytes).toBeGreaterThan(0);
    expect(details.glyphs_page1).toBeGreaterThan(0);
    expect(details.glyphs_page2).toBe(0);
    expect(details.selected_text).toContain("QUSOR");
    expect(details.saved_ok).toBe(true);
    const savedRange = details.saved_ranges.find(
        (range: any) => range.start === 0 && range.end === 5,
    );
    expect(typeof savedRange?.capture_id).toBe("number");
    expect(details.text_ok).toBe(true);
    expect(details.text).toContain("QUSOR");
}, 45000);

test("a malformed PDF is rejected without an import receipt", async () => {
    const app = await open("./pdf-import-harness.tsx");
    await app.press("label:import broken pdf");
    await app.waitFor("text:broken rejected", { timeout: 20000 });
}, 30000);

test("the Library workspace shows a Core PDF and its native page", async () => {
    const app = await open("./library-pdf-harness.tsx");
    await app.waitFor("text:本机 Core · 只显示资料元数据", {
        timeout: 20000,
    });
    const ready = app.texts().find((text) => text.startsWith("ready "));
    expect(ready).toBeDefined();
    const title = ready!.slice("ready ".length);
    await app.waitFor(`label:继续阅读${title}`, { timeout: 10000 });
    expect(app.enabled("text:继续")).toBe(true);
    expect(app.count("text:继续")).toBe(1);
    await app.press("text:继续");
    await app.waitFor(`label:《${title}》第 1 页原生页图`, {
        timeout: 20000,
    });
    expect(app.shows(`text:${title}`)).toBe(true);
    expect(app.shows("text:本机 Core · 原生页图 + 文字层")).toBe(true);
    expect(app.enabled("label:缩小资料页图")).toBe(false);
    await app.press("label:放大资料页图");
    await app.waitFor("text:125%");
    expect(app.enabled("label:缩小资料页图")).toBe(true);
    await app.press("label:资料页图适合宽度");
    await app.waitFor("text:100%");
    await app.press("label:下一页");
    await app.waitFor(`label:《${title}》第 2 页原生页图`, {
        timeout: 10000,
    });
}, 45000);
