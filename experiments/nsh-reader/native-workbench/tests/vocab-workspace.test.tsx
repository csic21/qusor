import { expect, open, test } from "nsh/test";

const cet4 = "label:选择词书 CET-4 示例";
const academic = "label:选择词书 学术写作 示例";
const abandonMeaning = "放弃；抛弃";
const abandonExample =
    "They had to abandon the plan when the funding was withdrawn.";
const abandonIpa = "/əˈbændən/";
const coherentUsIpa = "/koʊˈhɪrənt/";
const coherentUkIpa = "/kəʊˈhɪərənt/";
const missingVoice = "text:/当前宿主没有系统读音/";

async function vocabApp(width = 1280, height = 820) {
    const app = await open("../App.tsx", {
        platform: "macos",
        width,
        height,
    });
    await app.press("label:背单词");
    await app.waitFor("text:CET-4 示例");
    return app;
}

/// A card starts as a prompt: only the term and the answer action.
async function openCet4(app: Awaited<ReturnType<typeof vocabApp>>) {
    await app.press(cet4);
    await app.waitFor("text:abandon");
}

test("the workspace lists both sample wordbooks and marks the source", async () => {
    const app = await vocabApp();
    expect(app.texts()).toContain("CET-4 示例");
    expect(app.texts()).toContain("学术写作 示例");
    expect(app.texts()).toContain("2 本词书");
    await app.waitFor("text:样例数据 · 窗口内样例");
    await app.waitFor("text:选择一本词书");
});

test("a wordbook opens one term with the answer still hidden", async () => {
    const app = await vocabApp();
    await openCet4(app);
    expect(app.texts()).toContain("abandon");
    expect(app.texts()).not.toContain(abandonMeaning);
    expect(app.texts()).not.toContain(abandonExample);
    expect(app.shows("label:显示答案")).toBeTruthy();
    expect(app.shows("label:不认识")).toBeFalsy();
    expect(app.shows("label:模糊")).toBeFalsy();
    expect(app.shows("label:认识")).toBeFalsy();
    expect(app.shows("label:很熟")).toBeFalsy();
    expect(app.texts()).toContain("待复习 0");
    expect(app.texts()).toContain("今日新词 3");
});

test("a card shows its part of speech and sample phonetic before the answer", async () => {
    const app = await vocabApp();
    await openCet4(app);
    expect(app.texts()).toContain("v.");
    expect(app.texts()).toContain(abandonIpa);
    expect(app.shows("label:朗读 abandon")).toBeTruthy();
    expect(app.shows("label:美音")).toBeTruthy();
    expect(app.shows("label:英音")).toBeTruthy();
    // The phonetic must not answer the card: the meaning still waits for reveal.
    expect(app.texts()).not.toContain(abandonMeaning);
});

test("the accent switch changes the phonetic and still hides the answer", async () => {
    const app = await vocabApp();
    await app.press(academic);
    await app.waitFor("text:coherent");
    expect(app.texts()).toContain(coherentUsIpa);
    await app.press("label:英音");
    await app.waitFor(`text:${coherentUkIpa}`);
    expect(app.texts()).not.toContain(coherentUsIpa);
    expect(app.texts()).not.toContain("连贯的；一致的");
});

test("a window without a system voice says so instead of pretending", async () => {
    const app = await vocabApp();
    await openCet4(app);
    await app.press("label:朗读 abandon");
    await app.waitFor(missingVoice);
    expect(app.texts()).toContain("abandon");
    expect(app.shows("label:显示答案")).toBeTruthy();
});

test("R reads the current word and the revealed example offers its own reading", async () => {
    const app = await vocabApp();
    await openCet4(app);
    await app.keyboard.press("r");
    await app.waitFor(missingVoice);
    await app.press("label:显示答案");
    await app.waitFor(`text:${abandonMeaning}`);
    expect(app.shows("label:朗读例句")).toBeTruthy();
    await app.press("label:朗读例句");
    await app.waitFor(missingVoice);
});

test("reading preferences announce their state and persist in app storage", async () => {
    const app = await vocabApp();
    expect(app.shows("label:自动朗读：关")).toBeTruthy();
    expect(app.shows("label:慢速朗读：关")).toBeTruthy();
    await app.press("label:自动朗读：关");
    await app.waitFor("label:自动朗读：开");
    await app.press("label:慢速朗读：关");
    await app.waitFor("label:慢速朗读：开");
    await app.waitUntil(() => app.storage()["vocab.autoRead"] === "1");
    await app.waitUntil(() => app.storage()["vocab.slowRead"] === "1");
});

test("a stored reading preference comes back on the next open", async () => {
    const app = await open("../App.tsx", {
        platform: "macos",
        width: 1280,
        height: 820,
        storage: { "vocab.autoRead": "1", "vocab.slowRead": "1" },
    });
    await app.press("label:背单词");
    await app.waitFor("text:CET-4 示例");
    await app.waitFor("label:自动朗读：开");
    expect(app.shows("label:慢速朗读：开")).toBeTruthy();
});

test("automatic reading stays quiet when the window has no voice", async () => {
    const app = await open("../App.tsx", {
        platform: "macos",
        width: 1280,
        height: 820,
        storage: { "vocab.autoRead": "1" },
    });
    await app.press("label:背单词");
    await app.waitFor("text:CET-4 示例");
    await app.press(cet4);
    await app.waitFor("text:abandon");
    await app.press("label:显示答案");
    await app.press("label:认识");
    await app.waitFor("text:benefit");
    expect(app.texts()).toContain("已记录：认识 · 样例队列不写入 Core");
    expect(app.texts()).not.toContain(
        "当前宿主没有系统读音；先照音标读一遍再揭晓。",
    );
    // An explicit press still says why nothing is heard.
    await app.press("label:朗读 benefit");
    await app.waitFor(missingVoice);
});

test("showing the answer reveals the meaning, the example and four grades", async () => {
    const app = await vocabApp();
    await openCet4(app);
    await app.press("label:显示答案");
    await app.waitFor(`text:${abandonMeaning}`);
    expect(app.texts()).toContain(abandonExample);
    expect(app.shows("label:显示答案")).toBeFalsy();
    expect(app.shows("label:不认识")).toBeTruthy();
    expect(app.shows("label:模糊")).toBeTruthy();
    expect(app.shows("label:认识")).toBeTruthy();
    expect(app.shows("label:很熟")).toBeTruthy();
});

test("a grade advances to a hidden next card and moves the counters", async () => {
    const app = await vocabApp();
    await openCet4(app);
    await app.press("label:显示答案");
    await app.waitFor(`text:${abandonMeaning}`);
    await app.press("label:认识");
    await app.waitFor("text:benefit");
    expect(app.texts()).toContain("今日新词 2");
    expect(app.texts()).toContain("待复习 0");
    expect(app.texts()).toContain("已记录：认识 · 样例队列不写入 Core");
    expect(app.texts()).not.toContain("abandon");
    expect(app.texts()).not.toContain("好处；使受益");
    expect(app.shows("label:显示答案")).toBeTruthy();
    expect(app.shows("label:认识")).toBeFalsy();
});

test("grading a card lowers the new-word count in the wordbook row", async () => {
    const app = await vocabApp();
    await openCet4(app);
    expect(app.texts()).toContain("共 3 · 待复习 0 · 新词 3");
    await app.press("label:显示答案");
    await app.press("label:认识");
    await app.waitFor("text:benefit");
    expect(app.texts()).toContain("共 3 · 待复习 0 · 新词 2");
    expect(app.texts()).toContain("共 2 · 待复习 0 · 新词 2");
});

test("space shows the answer and only then can digits grade", async () => {
    const app = await vocabApp();
    await openCet4(app);
    await app.keyboard.press("1");
    expect(app.shows("label:认识")).toBeFalsy();
    expect(app.texts()).toContain("abandon");
    await app.keyboard.press("space");
    await app.waitFor(`text:${abandonMeaning}`);
    await app.keyboard.press("1");
    await app.waitFor("text:benefit");
    expect(app.texts()).toContain("已记录：不认识 · 样例队列不写入 Core");
});

test("clearing the queue shows the done state instead of a hidden card", async () => {
    const app = await vocabApp();
    await app.press(cet4);
    for (const term of ["abandon", "benefit", "candidate"]) {
        await app.waitFor(`text:${term}`);
        await app.press("label:显示答案");
        await app.press("label:认识");
    }
    await app.waitFor("text:今天完成");
    expect(app.texts()).toContain("今日新词 0");
    expect(app.texts()).toContain("共 3 · 待复习 0 · 新词 0");
    expect(app.shows("label:认识")).toBeFalsy();
    expect(app.shows("label:显示答案")).toBeFalsy();
});

test("the second wordbook has its own two cards", async () => {
    const app = await vocabApp();
    await app.press(academic);
    await app.waitFor("text:coherent");
    expect(app.texts()).toContain("今日新词 2");
    await app.press("label:显示答案");
    await app.press("label:很熟");
    await app.waitFor("text:cite");
    expect(app.texts()).toContain("今日新词 1");
    expect(app.texts()).toContain("共 2 · 待复习 0 · 新词 1");
    await app.press("label:显示答案");
    await app.press("label:模糊");
    await app.waitFor("text:今天完成");
});

test("the 900×680 layout still selects a wordbook and grades a card", async () => {
    const app = await vocabApp(900, 680);
    await app.press(academic);
    await app.waitFor("text:coherent");
    expect(app.shows("label:显示答案")).toBeTruthy();
    expect(app.shows("label:自动朗读：关")).toBeTruthy();
    await app.press("label:显示答案");
    expect(app.shows("label:很熟")).toBeTruthy();
    await app.press("label:很熟");
    await app.waitFor("text:cite");
    expect(app.texts()).toContain("今日新词 1");
});

test("returning and re-entering keeps the position with the answer hidden", async () => {
    const app = await vocabApp();
    await openCet4(app);
    await app.press("label:显示答案");
    await app.waitFor(`text:${abandonMeaning}`);
    await app.press("label:认识");
    await app.waitFor("text:benefit");
    await app.press("label:返回产品设计 · 概览");
    await app.waitFor("text:产品设计");
    await app.press("label:背单词");
    await app.waitFor("text:benefit");
    expect(app.texts()).toContain("今日新词 2");
    expect(app.texts()).toContain("CET-4 示例");
    expect(app.shows("label:显示答案")).toBeTruthy();
    expect(app.shows("label:认识")).toBeFalsy();
    await app.press("label:显示答案");
    await app.press("label:认识");
    await app.waitFor("text:candidate");
    expect(app.texts()).toContain("今日新词 1");
});
