import { signal } from "nsh";
import { CompositionHost } from "qusor/pdf-surface";
import { PluginSlotView } from "./PluginSlotView";
import { type PluginPackage, type PluginWorkspace } from "./opener";

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
        if (raw.kind === "ui_slot") {
            const contributionId = raw.id;
            const contributionTitle = raw.title;
            workspaces.push({
                id: contributionId,
                package: raw.package,
                title: contributionTitle,
                kind: "扩展",
                role: raw.kind,
                icon: "plug",
                description: "声明式原生扩展工作区",
                render: (props) => (
                    <PluginSlotView
                        contributionId={contributionId}
                        title={contributionTitle}
                        compact={props.compact}
                    />
                ),
            });
        }
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
