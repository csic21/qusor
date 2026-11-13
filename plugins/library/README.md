# Library package

This ordinary signed package qualifies the Host API 0.7 `library_workspace` and `library.read` path. Its entrypoint is an empty Wasm module and the app does not execute it yet; the native Library workspace reads bounded document metadata and saved selection ranges through the trusted Host adapter.

The package does not receive original PDF bytes, filesystem paths, note bodies or database access.
