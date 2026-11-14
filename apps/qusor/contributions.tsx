// Shared shell views and generic plugin slot entrypoints for `App.tsx` and test harnesses.
export { Diagram, draft, entries, type Entry } from "./shell/entries";
export {
    setPluginWorkspaceOpener,
    type PluginPackage,
    type PluginWorkspace,
} from "./shell/opener";
export {
    bootWorkspaceCatalog,
    pluginPackages,
    pluginWorkspaces,
    refreshWorkspaceCatalog,
    setWorkspacePackageEnabled,
} from "./shell/catalog";
export { PluginSlotView } from "./shell/PluginSlotView";
