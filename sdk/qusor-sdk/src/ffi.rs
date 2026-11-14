#![cfg_attr(not(target_arch = "wasm32"), forbid(unsafe_code))]
#![cfg_attr(target_arch = "wasm32", allow(unsafe_code))]

use std::sync::Mutex;

/// Maximum reply buffer allocated for a single `qusor.call` broker invocation.
/// Bounded well below `MobileStandard` (1 MiB) and `DesktopStandard` (2 MiB).
pub const DEFAULT_HOST_CALL_CAPACITY: usize = 128 * 1024;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "qusor")]
unsafe extern "C" {
    fn call(ptr: i32, len: i32, out_ptr: i32, out_cap: i32) -> i32;
}

static INPUT_BUFFER: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static OUTPUT_BUFFER: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Allocates a bounded guest input buffer reused across turns.
pub fn alloc_input(len: i32) -> i32 {
    if len <= 0 {
        return 0;
    }
    let Ok(mut guard) = INPUT_BUFFER.lock() else {
        return 0;
    };
    *guard = vec![0u8; len as usize];
    guard.as_mut_ptr() as i32
}

/// Takes the host-written input bytes for the current turn.
pub fn take_input(_ptr: i32, len: i32) -> Vec<u8> {
    if len <= 0 {
        return Vec::new();
    }
    let Ok(mut guard) = INPUT_BUFFER.lock() else {
        return Vec::new();
    };
    let take_len = (len as usize).min(guard.len());
    let out = guard[..take_len].to_vec();
    guard.clear();
    out
}

/// Stores the serialized turn response and packs `(ptr, len)` into an `i64`.
pub fn pack_output(bytes: Vec<u8>) -> i64 {
    let Ok(mut guard) = OUTPUT_BUFFER.lock() else {
        return 0;
    };
    *guard = bytes;
    let ptr = guard.as_ptr() as i32;
    let len = i32::try_from(guard.len()).unwrap_or(0);
    ((ptr as u32 as u64) | ((len as u32 as u64) << 32)) as i64
}

/// Calls the QUSOR host broker through the `qusor.call` import.
#[cfg(target_arch = "wasm32")]
pub fn broker_call(request: &[u8]) -> Result<Vec<u8>, String> {
    let req_len =
        i32::try_from(request.len()).map_err(|_| "request exceeds i32 length bound".to_owned())?;
    let mut response = vec![0u8; DEFAULT_HOST_CALL_CAPACITY];
    let out_cap = i32::try_from(response.len())
        .map_err(|_| "response buffer exceeds i32 capacity".to_owned())?;
    // SAFETY: `request` is valid for `req_len` readable bytes and `response` is
    // valid for `out_cap` writable bytes across this synchronous Wasm host call.
    let written = unsafe {
        call(
            request.as_ptr() as i32,
            req_len,
            response.as_mut_ptr() as i32,
            out_cap,
        )
    };
    if written < 0 {
        return Err(format!("host call rejected with status {written}"));
    }
    let written_len = written as usize;
    if written_len > response.len() {
        return Err("host response length exceeded guest buffer".to_owned());
    }
    response.truncate(written_len);
    Ok(response)
}

/// Non-wasm32 stub; host tests inject an explicit `HostTransport`.
#[cfg(not(target_arch = "wasm32"))]
pub fn broker_call(_request: &[u8]) -> Result<Vec<u8>, String> {
    Err("qusor.call FFI transport is only available on wasm32 targets".to_owned())
}
