import { type JSX } from "nsh";

export type PluginWorkspace = {
    id: string;
    package: string;
    title: string;
    kind: string;
    role: string;
    icon: string;
    description: string;
    render: (props: { compact: boolean }) => JSX.Element;
};
export type PluginPackage = {
    id: string;
    title: string;
    version: string;
    enabled: boolean;
};
let pluginWorkspaceOpener: ((role: string) => boolean) | null = null;
export function setPluginWorkspaceOpener(
    opener: (role: string) => boolean,
) {
    pluginWorkspaceOpener = opener;
}

export function openPluginWorkspace(role: string): boolean {
    return pluginWorkspaceOpener?.(role) ?? false;
}
