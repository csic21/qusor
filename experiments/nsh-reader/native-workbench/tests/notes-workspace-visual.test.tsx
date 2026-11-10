// Screenshots for the evidence record. Run with `--target macos`: a headless
// run draws nothing and would write blank images.
import { open, test } from "nsh/test";

const evidence = "../evidence/notes-workspace-ui";

test("capture notes workspace states", async () => {
    const app = await open("../App.tsx", {
        platform: "macos",
        width: 1280,
        height: 820,
    });
    await app.press("label:想法");
    await app.waitFor("text:全部想法");
    await app.screenshot(`${evidence}/list.png`);

    await app.press("label:《准原生应用的秩序》第 3 页：原生界面里的层级");
    await app.waitFor("text:我的想法");
    await app.screenshot(`${evidence}/detail.png`);

    await app.press("label:返回想法列表");
    await app.press("label:《工具退到一旁》第 8 页：工具应该退到一旁");
    await app.waitFor("text:列表只显示开头，完整内容仍保存在本机。");
    await app.screenshot(`${evidence}/truncated.png`);

    await app.press("label:读取完整想法内容");
    await app.waitUntil(() =>
        app.texts().some((text) => text.includes("工具应该像书架一样")),
    );
    await app.screenshot(`${evidence}/full-body.png`);
});
