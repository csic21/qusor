export type PluginNavigation = {
    route: string;
    launch_token: string;
    payload: Record<string, unknown>;
};

export type PluginOpenResult =
    | { ok: true; workspaceId: string }
    | { ok: false; error: string };

export type SourceReturn = {
    workspaceId: string;
    title: string;
    navigation: PluginNavigation | null;
};

export type SourceReturnResolution =
    | {
          ok: true;
          kind: "workspace";
          workspaceId: string;
          navigation: PluginNavigation | null;
      }
    | { ok: true; kind: "origin" }
    | { ok: false; reason: "missing" | "no-fallback" };

type ParsedSourceAction =
    | { ok: true; navigation: PluginNavigation }
    | { ok: false; error: string };

function parseSafeInteger(value: string): number | null {
    if (!/^(0|[1-9][0-9]*)$/.test(value)) return null;
    const parsed = Number(value);
    return Number.isSafeInteger(parsed) ? parsed : null;
}

export function parseOpenSourceAction(action: string): ParsedSourceAction {
    const parts = action.split(":");
    if (parts.length !== 5 || parts[0] !== "open_source") {
        return {
            ok: false,
            error: "插件请求了无效的原文跳转动作，已阻止跳转。",
        };
    }
    if (parts[1] !== "pdf" && parts[1] !== "epub") {
        return {
            ok: false,
            error: "插件请求了不支持的原文格式，已阻止跳转。",
        };
    }
    const documentId = parseSafeInteger(parts[2]);
    const sectionIndex = parseSafeInteger(parts[3]);
    const captureId = parseSafeInteger(parts[4]);
    if (
        documentId === null ||
        documentId <= 0 ||
        sectionIndex === null ||
        captureId === null ||
        captureId <= 0
    ) {
        return {
            ok: false,
            error: "插件请求了无效的原文定位，已阻止跳转。",
        };
    }
    return {
        ok: true,
        navigation: {
            route: "source_capture",
            launch_token: "",
            payload: {
                format: parts[1],
                document_id: documentId,
                section_index: sectionIndex,
                capture_id: captureId,
            },
        },
    };
}

export function sourceReturnLabel(
    source: SourceReturn | null,
    fallback = "返回",
): string {
    const title = source?.title.trim() ?? "";
    return title ? `返回${title}` : fallback;
}

export function resolveSourceReturn(
    source: SourceReturn | null,
    workspaces: ReadonlyArray<{ id: string }>,
    origin: { active: string; library: boolean } | null,
): SourceReturnResolution {
    if (!source) return { ok: false, reason: "missing" };
    if (workspaces.some((workspace) => workspace.id === source.workspaceId)) {
        return {
            ok: true,
            kind: "workspace",
            workspaceId: source.workspaceId,
            navigation: source.navigation,
        };
    }
    if (origin) {
        return { ok: true, kind: "origin" };
    }
    return { ok: false, reason: "no-fallback" };
}
