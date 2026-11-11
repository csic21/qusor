// Screenshots for local review. Run with `--target macos`: a headless run draws
// nothing and would write blank images. The record kept in the repository is the
// one `prepare.py --evidence vocab` writes through VocabHost.capture; these files
// stay under the build directory so a test run never needs a new evidence folder.
import { open, test } from "nsh/test";

const evidence = "../target";

test("capture vocab workspace states", async () => {
    const app = await open("../App.tsx", {
        platform: "macos",
        width: 1280,
        height: 820,
    });
    await app.press("label:背单词");
    await app.waitFor("text:CET-4 示例");
    await app.screenshot(`${evidence}/vocab-decks.png`);

    await app.press("label:选择词书 CET-4 示例");
    await app.waitFor("text:abandon");
    await app.screenshot(`${evidence}/vocab-study.png`);

    await app.press("label:显示答案");
    await app.waitFor("text:放弃；抛弃");
    await app.screenshot(`${evidence}/vocab-revealed.png`);

    await app.press("label:认识");
    await app.waitFor("text:benefit");
    await app.screenshot(`${evidence}/vocab-reviewed.png`);

    for (const term of ["benefit", "candidate"]) {
        await app.waitFor(`text:${term}`);
        await app.press("label:显示答案");
        await app.press("label:认识");
    }
    await app.waitFor("text:今天完成");
    await app.screenshot(`${evidence}/vocab-done.png`);
});

test("capture the 900×680 layout", async () => {
    const app = await open("../App.tsx", {
        platform: "macos",
        width: 900,
        height: 680,
    });
    await app.press("label:背单词");
    await app.waitFor("text:CET-4 示例");
    await app.press("label:选择词书 学术写作 示例");
    await app.waitFor("text:coherent");
    await app.screenshot(`${evidence}/vocab-compact.png`);
    await app.press("label:显示答案");
    await app.waitFor("text:连贯的；一致的");
    await app.screenshot(`${evidence}/vocab-compact-revealed.png`);
});
