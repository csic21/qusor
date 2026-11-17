import { expect, test } from "nsh/test";
import {
    parseOpenSourceAction,
    resolveSourceReturn,
    sourceReturnLabel,
} from "../shell/plugin_navigation";
import {
    openPluginWorkspace,
    setPluginWorkspaceOpener,
    type PluginNavigation,
} from "../shell/opener";

const safeMax = Number.MAX_SAFE_INTEGER;

test("open_source actions accept exact safe integers", () => {
    for (const [action, expected] of [
        [
            "open_source:pdf:1:0:2",
            {
                format: "pdf",
                document_id: 1,
                section_index: 0,
                capture_id: 2,
            },
        ],
        [
            "open_source:epub:3:7:9",
            {
                format: "epub",
                document_id: 3,
                section_index: 7,
                capture_id: 9,
            },
        ],
        [
            `open_source:pdf:${safeMax}:${safeMax}:${safeMax}`,
            {
                format: "pdf",
                document_id: safeMax,
                section_index: safeMax,
                capture_id: safeMax,
            },
        ],
    ] as const) {
        expect(parseOpenSourceAction(action)).toEqual({
            ok: true,
            navigation: {
                route: "source_capture",
                launch_token: "",
                payload: expected,
            },
        });
    }
});

test("open_source actions reject invalid ids and out-of-range values", () => {
    for (const action of [
        "open_source:pdf:0:0:2",
        "open_source:pdf:1:0:0",
        "open_source:pdf:-1:0:2",
        "open_source:pdf:1:-1:2",
        "open_source:pdf:1:0:-1",
        "open_source:pdf:1.5:0:2",
        "open_source:pdf:1:0:2.5",
        "open_source:pdf:1:0:2.0",
        "open_source:pdf:9007199254740992:0:2",
        "open_source:pdf:1:9007199254740992:2",
        "open_source:pdf:1:0:9007199254740992",
        "open_source:pdf:01:0:2",
        "open_source:pdf:1:00:2",
        "open_source:pdf:1:0:02",
        "open_source:pdf:1e3:0:2",
        "open_source:pdf:+1:0:2",
        "open_source:pdf: 1:0:2",
        "open_source:pdf:1: 0:2",
        "open_source:pdf:1:0:2 ",
        "open_source:pdf::0:2",
        "open_source:pdf:1::2",
        "open_source:pdf:1:0:",
    ]) {
        expect(parseOpenSourceAction(action)).toEqual({
            ok: false,
            error: "插件请求了无效的原文定位，已阻止跳转。",
        });
    }
});

test("open_source actions reject formats, prefixes, and segment counts", () => {
    for (const action of [
        "open_source:docx:1:0:2",
        "open_source:PDF:1:0:2",
        "open_source:pdf:1:0",
        "open_source:pdf:1:0:2:3",
        "open_source:pdf:1:0:2:",
        "open_source::1:0:2",
        "open_source",
        "open_other:pdf:1:0:2",
        "open_source :pdf:1:0:2",
    ]) {
        const result = parseOpenSourceAction(action);
        expect(result.ok).toBe(false);
        if (!result.ok) expect(result.error.length).toBeGreaterThan(0);
    }
});

test("source return labels trim titles and fall back", () => {
    const navigation: PluginNavigation = {
        route: "source_capture",
        launch_token: "token",
        payload: {},
    };
    expect(
        sourceReturnLabel({
            workspaceId: "notes",
            title: " 想法 ",
            navigation,
        }),
    ).toBe("返回想法");
    expect(
        sourceReturnLabel(
            { workspaceId: "notes", title: "   ", navigation: null },
            "返回来源",
        ),
    ).toBe("返回来源");
    expect(sourceReturnLabel(null, "返回来源")).toBe("返回来源");
});

test("source return resolution distinguishes workspace, origin, and missing", () => {
    const navigation: PluginNavigation = {
        route: "source_capture",
        launch_token: "token",
        payload: { capture_id: 2 },
    };
    const source = {
        workspaceId: "notes",
        title: "想法",
        navigation,
    };
    expect(
        resolveSourceReturn(source, [{ id: "reader" }, { id: "notes" }], null),
    ).toEqual({
        ok: true,
        kind: "workspace",
        workspaceId: "notes",
        navigation,
    });
    expect(
        resolveSourceReturn(source, [{ id: "reader" }], {
            active: "overview",
            library: false,
        }),
    ).toEqual({ ok: true, kind: "origin" });
    expect(resolveSourceReturn(source, [{ id: "reader" }], null)).toEqual({
        ok: false,
        reason: "no-fallback",
    });
    expect(resolveSourceReturn(null, [], null)).toEqual({
        ok: false,
        reason: "missing",
    });
});

test("the opener propagates results and navigation", () => {
    const navigation: PluginNavigation = {
        route: "source_capture",
        launch_token: "token",
        payload: { capture_id: 2 },
    };
    let received: PluginNavigation | null | undefined;
    setPluginWorkspaceOpener((role, value) => {
        expect(role).toBe("ui_slot");
        received = value;
        return { ok: true, workspaceId: "reader" };
    });
    expect(openPluginWorkspace("ui_slot", navigation)).toEqual({
        ok: true,
        workspaceId: "reader",
    });
    expect(received).toBe(navigation);

    setPluginWorkspaceOpener(() => ({
        ok: false,
        error: "未找到可用的阅读插件，原文未打开。",
    }));
    expect(openPluginWorkspace("ui_slot", navigation)).toEqual({
        ok: false,
        error: "未找到可用的阅读插件，原文未打开。",
    });
});
