// Feature workspaces and shared shell views split under `./features/*` and `./shell/*`.
// This barrel preserves the public entrypoints for `App.tsx` and test harnesses.
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
export { captureNotesEvidence, refreshNotes } from "./features/notes/NotesWorkspace";
export { LibraryWorkspace } from "./features/library/LibraryWorkspace";
