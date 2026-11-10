import { expect, open, test } from "nsh/test";

const firstRow = "label:《准原生应用的秩序》第 3 页：原生界面里的层级";
const secondRow = "label:《工具退到一旁》第 8 页：工具应该退到一旁";

async function notesApp(width = 1280, height = 820) {
    const app = await open("../App.tsx", {
        platform: "macos",
        width,
        height,
    });
    await app.press("label:想法");
    await app.waitFor("text:全部想法");
    return app;
}

test("a row opens its detail, and Escape returns to the list", async () => {
    const app = await notesApp();
    expect(app.texts()).toContain("4 条想法");
    await app.press(firstRow);
    await app.waitFor("text:我的想法");
    expect(app.texts()).toContain("原生界面里的层级");
    expect(app.texts()).toContain("打开所在页");
    await app.keyboard.press("escape");
    await app.waitFor("text:4 条想法");
});

test("the row context menu offers read actions and copies a citation", async () => {
    const app = await notesApp();
    await app.rightClick(firstRow);
    expect(app.menu()).toEqual([
        { label: "打开所在页", disabled: false },
        { label: "复制引文", disabled: false },
        { label: "复制想法", disabled: false },
        { label: "复制引用", disabled: false },
        { label: "内容已完整", disabled: true },
        { label: "编辑想法（下一增量）", disabled: true },
        { label: "删除想法（需要历史与撤销）", disabled: true },
    ]);
    await app.chooseMenuItem("复制引用");
    expect(app.clipboard()).toBe(
        "> 插件拥有自己的工作方式，用户始终拥有回到上层工作区的路径。\n\n— 《准原生应用的秩序》第 3 页",
    );
    await app.press("label:返回想法列表");
    await app.waitFor("text:4 条想法");
    await app.rightClick(firstRow);
    await app.chooseMenuItem("复制想法");
    expect(app.clipboard()).toBe(
        "宿主返回和插件内部返回要写成两个不同的目的地。进入插件后，身份与来源不能一起消失。",
    );
});

test("the missing source row says so and cannot open a page", async () => {
    const app = await notesApp();
    const missing = "label:《已移走的资料》第 2 页：原件缺失时的诚实反馈";
    await app.rightClick(missing);
    expect(app.menu()).toEqual([
        { label: "打开所在页", disabled: true },
        { label: "复制引文", disabled: false },
        { label: "复制想法", disabled: false },
        { label: "复制引用", disabled: false },
        { label: "读取完整内容", disabled: false },
        { label: "编辑想法（下一增量）", disabled: true },
        { label: "删除想法（需要历史与撤销）", disabled: true },
    ]);
    await expect(() => app.chooseMenuItem("打开所在页")).rejects.toMatchObject({
        code: "ERR_NSH_TEST_DISABLED",
    });
    await app.closeMenu();
    await app.waitFor(
        "text:原件缺失，无法定位到这一页；引文与想法仍可复制。",
    );
    expect(app.enabled("label:原件缺失，无法定位")).toBeFalsy();
});

test("a truncated note reads its complete body on request", async () => {
    const app = await notesApp();
    await app.press(secondRow);
    await app.waitFor("text:列表只显示开头，完整内容仍保存在本机。");
    await app.press("label:读取完整想法内容");
    await app.waitUntil(() =>
        app.texts().some((text) => text.includes("工具应该像书架一样")),
    );
    expect(app.shows("label:读取完整想法内容")).toBeFalsy();
});

test("keyboard moves the selection and copies the focused note", async () => {
    const app = await notesApp();
    await app.keyboard.press("down");
    await app.waitFor("text:我的想法");
    expect(app.texts()).toContain("原生界面里的层级");
    await app.keyboard.press("mod+c");
    expect(app.clipboard()).toBe(
        "宿主返回和插件内部返回要写成两个不同的目的地。进入插件后，身份与来源不能一起消失。",
    );
    await app.keyboard.press("escape");
    await app.waitFor("text:4 条想法");
});

test("the narrow layout returns to the list and keeps the filter", async () => {
    const app = await notesApp(900, 680);
    await app.press(firstRow);
    await app.waitFor("text:我的想法");
    await app.press("label:返回想法列表");
    await app.waitFor("text:4 条想法");
    expect(app.texts()).toContain("全部想法");
    await app.press("label:显示想法导航");
    await app.press("label:显示当前资料想法");
    await app.waitFor("text:2 条想法");
});
