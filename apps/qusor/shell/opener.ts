import { type JSX } from "nsh";
import {
    type PluginNavigation,
    type PluginOpenResult,
} from "./plugin_navigation";

export {
    parseOpenSourceAction,
    resolveSourceReturn,
    sourceReturnLabel,
    type PluginNavigation,
    type PluginOpenResult,
    type SourceReturn,
} from "./plugin_navigation";

export type PluginWorkspace = {
    id: string;
    package: string;
    title: string;
    kind: string;
    role: string;
    icon: string;
    description: string;
    render: (props: {
        compact: boolean;
        navigation: PluginNavigation | null;
        onOpenSource: (navigation: PluginNavigation) => PluginOpenResult;
    }) => JSX.Element;
};
export type PluginPackage = {
    id: string;
    title: string;
    version: string;
    enabled: boolean;
};
let pluginWorkspaceOpener:
    | ((role: string, navigation?: PluginNavigation | null) => PluginOpenResult)
    | null = null;
export function setPluginWorkspaceOpener(
    opener: (role: string, navigation?: PluginNavigation | null) => PluginOpenResult,
) {
    pluginWorkspaceOpener = opener;
}

export function openPluginWorkspace(
    role: string,
    navigation?: PluginNavigation | null,
): PluginOpenResult {
    return (
        pluginWorkspaceOpener?.(role, navigation) ?? {
            ok: false,
            error: "当前宿主没有可用的插件工作区入口。",
        }
    );
}
