# Reader preview package

This package qualifies the Host API 0.2 `reader_workspace` contribution path. Its entrypoint is an empty Wasm module and the app does not execute it yet; the native reader workspace remains the existing in-window fixture renderer.

The package is built outside the repository through the exported SDK by `build.py`. It does not grant Core access, install state, or a production reader capability.
