import { signal } from "nsh";
import { CompositionHost } from "qusor/pdf-surface";
import { LibraryWorkspace } from "../features/library/LibraryWorkspace";
import { NotesWorkspace } from "../features/notes/NotesWorkspace";
import { ReadingWorkspace } from "../features/reader/ReadingWorkspace";
import { VocabWorkspace } from "../features/vocab/VocabWorkspace";
import { type PluginPackage, type PluginWorkspace } from "./opener";

type WorkspaceRenderer = {
    kind: string;
    icon: string;
    description: string;
    render: PluginWorkspace["render"];
};

const workspaceRenderers: Record<string, WorkspaceRenderer> = {
    notes_workspace: {
        kind: "想法",
        icon: "pen",
        description: "引用、个人观点和按资料浏览",
        render: (props) => <NotesWorkspace compact={props.compact} />,
    },
    vocab_workspace: {
        kind: "词汇",
        icon: "book",
        description: "词书、复习安排与四档评分",
        render: (props) => <VocabWorkspace compact={props.compact} />,
    },
    reader_workspace: {
        kind: "阅读",
        icon: "book",
        description: "原生阅读界面资格；窗口内样例不写入 Core",
        render: (props) => <ReadingWorkspace compact={props.compact} />,
    },
    library_workspace: {
        kind: "资料库",
        icon: "book",
        description: "本机 Core 的 PDF 元数据列表",
        render: (props) => <LibraryWorkspace compact={props.compact} />,
    },
};

const installedWorkspaces = signal<PluginWorkspace[]>([]);
const installedPackages = signal<PluginPackage[]>([]);
let workspacesBooted = false;

function applyWorkspaceCatalog(value: any) {
    if (!Array.isArray(value?.workspaces)) return false;
    const workspaces: PluginWorkspace[] = [];
    for (const raw of value.workspaces) {
        if (
            typeof raw?.id !== "string" ||
            typeof raw?.package !== "string" ||
            typeof raw?.title !== "string" ||
            typeof raw?.kind !== "string"
        ) {
            continue;
        }
        const renderer = workspaceRenderers[raw.kind];
        if (!renderer) continue;
        workspaces.push({
            id: raw.id,
            package: raw.package,
            title: raw.title,
            kind: renderer.kind,
            role: raw.kind,
            icon: renderer.icon,
            description: renderer.description,
            render: renderer.render,
        });
    }
    installedWorkspaces.set(workspaces);
    return true;
}

export function refreshWorkspaceCatalog(): boolean {
    try {
        const response = JSON.parse(CompositionHost.workspaces());
        return response.ok === true && applyWorkspaceCatalog(response.value);
    } catch (_) {
        // A headless shell has no native contribution catalog.
        return false;
    }
}

function refreshPackageCatalog(): boolean {
    try {
        const response = JSON.parse(CompositionHost.packages());
        if (!response.ok || !Array.isArray(response.value?.packages))
            return false;
        installedPackages.set(
            response.value.packages.filter(
                (raw: any) =>
                    typeof raw?.id === "string" &&
                    typeof raw?.title === "string" &&
                    typeof raw?.version === "string" &&
                    typeof raw?.enabled === "boolean",
            ),
        );
        return true;
    } catch (_) {
        return false;
    }
}

export function setWorkspacePackageEnabled(
    packageId: string,
    enabled: boolean,
): boolean {
    try {
        const response = JSON.parse(
            CompositionHost.setPackageEnabled(packageId, enabled),
        );
        if (!response.ok || !applyWorkspaceCatalog(response.value)) return false;
        refreshPackageCatalog();
        return true;
    } catch (_) {
        return false;
    }
}

export function bootWorkspaceCatalog() {
    if (workspacesBooted) return;
    workspacesBooted = true;
    refreshWorkspaceCatalog();
    refreshPackageCatalog();
}

export function pluginWorkspaces(): PluginWorkspace[] {
    return installedWorkspaces();
}

export function pluginPackages(): PluginPackage[] {
    return installedPackages();
}
