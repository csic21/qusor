//! M-1 Wasmi probe. No WASI, real user data, knowledge writes or production integration.
//! Fuel is an instruction budget, not a deadline for native host code. The CLI supervisor
//! supplies a separate process watchdog. Neither mechanism is an OS permission sandbox.
use extension_api::{Capability, CapabilityRequest, ErrorCode, HOST_API, Manifest, Resource};
use extension_host::{Grant, Host, InstallationId, Session};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use wasmi::{Caller, Engine, Linker, Module, Store, StoreLimits, TypedResumableCall};

pub mod managed_runtime;
pub mod private_runtime;

use runtime_wasm::{bounded_runtime_config, bounded_runtime_limits};

pub const MAX_WASM_BYTES: usize = 256 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 4096;
pub const SYNTHETIC_SELECTION: &str = "本地选区: Rust & Wasm.";

pub use extension_runtime_api::{ExecutionResult as RunResult, RuntimeError as ProbeError};
type Result<T> = std::result::Result<T, ProbeError>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format_version: u16,
    entry: String,
    sha256: String,
}

/// Frozen bytes read and hashed before compilation. Digest integrity is not publisher trust.
pub struct Package {
    pub manifest: Manifest,
    pub digest: String,
    wasm: Vec<u8>,
}

impl Package {
    /// Consume frozen bytes from a trusted package verifier without reopening an input path.
    /// The supplied digest identifies the verifier's complete package, not just this module.
    /// This constructor does not authenticate a publisher; that is the caller's responsibility.
    pub fn from_host_bytes(manifest: Manifest, digest: String, wasm: Vec<u8>) -> Result<Self> {
        manifest
            .validate()
            .map_err(|_| ProbeError::new("invalid_manifest"))?;
        if wasm.len() > MAX_WASM_BYTES {
            return Err(ProbeError::new("package_size_limit"));
        }
        if !wasm.starts_with(b"\0asm\x01\0\0\0") {
            return Err(ProbeError::new("invalid_wasm"));
        }
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ProbeError::new("invalid_package"));
        }
        Ok(Self {
            manifest,
            digest,
            wasm,
        })
    }

    pub fn load(directory: &Path) -> Result<Self> {
        let envelope: Envelope =
            serde_json::from_slice(&read_bounded(&directory.join("package.json"), 4096)?)
                .map_err(|_| ProbeError::new("invalid_package"))?;
        if envelope.format_version != 1
            || envelope.entry != "module.wasm"
            || envelope.sha256.len() != 64
            || !envelope
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ProbeError::new("invalid_package"));
        }
        let manifest = Manifest::from_slice(&read_bounded(
            &directory.join("manifest.json"),
            extension_api::MAX_MANIFEST_BYTES,
        )?)
        .map_err(|_| ProbeError::new("invalid_manifest"))?;
        let wasm = read_bounded(&directory.join("module.wasm"), MAX_WASM_BYTES)?;
        if !wasm.starts_with(b"\0asm\x01\0\0\0") {
            return Err(ProbeError::new("invalid_wasm"));
        }
        let digest = format!("{:x}", Sha256::digest(&wasm));
        if digest != envelope.sha256 {
            return Err(ProbeError::new("integrity_mismatch"));
        }
        Ok(Self {
            manifest,
            digest,
            wasm,
        })
    }
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    // Static local fixtures only: this is not a race-resistant production package installer.
    let metadata = fs::symlink_metadata(path).map_err(|_| ProbeError::new("package_io"))?;
    if !metadata.file_type().is_file() {
        return Err(ProbeError::new("invalid_package_file"));
    }
    if metadata.len() > limit as u64 {
        return Err(ProbeError::new("package_size_limit"));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| ProbeError::new("package_io"))?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ProbeError::new("package_io"))?;
    if bytes.len() > limit {
        return Err(ProbeError::new("package_size_limit"));
    }
    Ok(bytes)
}

/// Trusted experiment configuration, never obtained from the guest package.
#[derive(Clone)]
pub struct Options {
    pub total_fuel: u64,
    pub slice_fuel: u64,
    pub deadline: Duration,
    pub host_calls: usize,
    /// Deliberate native blocking fault for the subprocess watchdog test only.
    pub block_host_call: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            total_fuel: 1_000_000,
            slice_fuel: 10_000,
            deadline: Duration::from_millis(250),
            host_calls: 8,
            block_host_call: false,
        }
    }
}
#[derive(Clone)]
pub struct Control {
    authority: Arc<Mutex<Host>>,
    installation: InstallationId,
    session: Session,
    cancelled: Arc<AtomicBool>,
    slices: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    request: CapabilityRequest,
    response: Arc<Vec<u8>>,
    expires_at: Option<u64>,
}
impl Control {
    pub fn cancel(&self) {
        // Serialize cancellation with bounded guest reads and result acceptance.
        let _guard = self
            .authority
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.cancelled.store(true, Ordering::SeqCst);
    }
    pub fn revoke(&self) -> Result<()> {
        self.authority
            .lock()
            .map_err(|_| ProbeError::new("host_state_poisoned"))?
            .replace_grants(self.installation, vec![])
            .map_err(policy_error)
    }
    pub fn slices(&self) -> usize {
        self.slices.load(Ordering::SeqCst)
    }
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    fn check(&self, started: Instant, deadline: Duration) -> Result<()> {
        let authority = self
            .authority
            .lock()
            .map_err(|_| ProbeError::new("host_state_poisoned"))?;
        self.check_locked(&authority, started, deadline)
    }
    fn check_locked(&self, authority: &Host, started: Instant, deadline: Duration) -> Result<()> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(ProbeError::new("cancelled"));
        }
        if started.elapsed() >= deadline {
            return Err(ProbeError::new("deadline_exceeded"));
        }
        if let Some(expiry) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| ProbeError::new("invalid_clock"))?
                .as_secs();
            if now >= expiry {
                return Err(ProbeError::new("grant_expired"));
            }
        }
        // Recheck lifecycle even while guest code loops without making a host call.
        authority
            .authorize(&self.session, self.request.clone())
            .map_err(policy_error)?;
        Ok(())
    }
}
fn selection_request() -> CapabilityRequest {
    CapabilityRequest {
        api_version: HOST_API,
        request_id: "runtime-check".into(),
        capability: Capability::SelectionRead,
        resource: Resource::Selection {
            vault_id: "probe-vault".into(),
            selection_id: "selection-1".into(),
        },
    }
}
fn policy_error(error: extension_api::ApiError) -> ProbeError {
    ProbeError::new(match error.code {
        ErrorCode::StaleContext => "stale_context",
        ErrorCode::UnsupportedVersion => "unsupported_version",
        ErrorCode::CapabilityUnavailable => "capability_unavailable",
        ErrorCode::PermissionDenied => "permission_denied",
        _ => "policy_rejected",
    })
}

/// Trusted adapter input, never a guest payload. No grants are created here.
/// The host/session must already authorize this exact response scope.
pub struct ExistingAuthority {
    pub host: Host,
    pub installation: InstallationId,
    pub session: Session,
    pub request: CapabilityRequest,
    pub response: Vec<u8>,
    pub expires_at: Option<u64>,
}

pub struct Probe {
    package: Package,
    options: Options,
    control: Control,
}
struct State {
    control: Control,
    options: Options,
    started: Instant,
    limits: StoreLimits,
    error: Option<ProbeError>,
}
impl Probe {
    pub fn new(package: Package, options: Options) -> Result<Self> {
        if options.slice_fuel == 0 || options.total_fuel == 0 || options.host_calls == 0 {
            return Err(ProbeError::new("invalid_options"));
        }
        let mut authority = Host::new(
            "probe-vault",
            [Capability::SelectionRead, Capability::CapturePropose],
        )
        .map_err(policy_error)?;
        let installation = authority
            .register(package.manifest.clone(), package.digest.clone())
            .map_err(policy_error)?;
        // Explicit trusted test grant; it is not inferred from manifest permissions.
        authority
            .replace_grants(
                installation,
                vec![Grant {
                    capability: Capability::SelectionRead,
                    resource: selection_request().resource,
                }],
            )
            .map_err(policy_error)?;
        authority.enable(installation).map_err(policy_error)?;
        let session = authority.open_session(installation).map_err(policy_error)?;
        Self::from_authority(
            package,
            options,
            ExistingAuthority {
                host: authority,
                installation,
                session,
                request: selection_request(),
                response: SYNTHETIC_SELECTION.as_bytes().to_vec(),
                expires_at: None,
            },
        )
    }

    /// Consume a previously authorized host/session without granting any capability.
    /// The caller provides bounded service output, not guest-controlled authority.
    pub fn from_authority(
        package: Package,
        options: Options,
        mut existing: ExistingAuthority,
    ) -> Result<Self> {
        if options.slice_fuel == 0
            || options.total_fuel == 0
            || options.host_calls == 0
            || existing.response.len() > MAX_RESPONSE_BYTES
        {
            return Err(ProbeError::new("invalid_options"));
        }
        if existing
            .host
            .manifest(existing.installation)
            .map_err(policy_error)?
            != &package.manifest
        {
            return Err(ProbeError::new("authority_package_mismatch"));
        }
        let permit = existing
            .host
            .authorize(&existing.session, existing.request.clone())
            .map_err(policy_error)?;
        existing
            .host
            .dispatch(permit, |context| {
                if context.installation_id != existing.installation
                    || context.package_id != package.manifest.package_id
                    || context.package_digest != package.digest
                {
                    return Err(extension_api::ApiError::new(
                        ErrorCode::PermissionDenied,
                        "authority package mismatch",
                    ));
                }
                Ok(())
            })
            .map_err(policy_error)?;
        let control = Control {
            authority: Arc::new(Mutex::new(existing.host)),
            installation: existing.installation,
            session: existing.session,
            cancelled: Arc::new(AtomicBool::new(false)),
            slices: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
            request: existing.request,
            response: Arc::new(existing.response),
            expires_at: existing.expires_at,
        };
        Ok(Self {
            package,
            options,
            control,
        })
    }
    pub fn control(&self) -> Control {
        self.control.clone()
    }

    pub fn run(self) -> Result<RunResult> {
        let started = Instant::now();
        self.control.check(started, self.options.deadline)?;
        let engine = Engine::new(&bounded_runtime_config());
        let module = Module::new(&engine, &self.package.wasm)
            .map_err(|_| ProbeError::new("invalid_module"))?;
        if module
            .imports()
            .any(|import| import.module() != "qusor" || import.name() != "call")
        {
            return Err(ProbeError::new("unknown_import"));
        }
        self.control.check(started, self.options.deadline)?;
        let limits = bounded_runtime_limits();
        let mut store = Store::new(
            &engine,
            State {
                control: self.control.clone(),
                options: self.options.clone(),
                started,
                limits,
                error: None,
            },
        );
        store.limiter(|state| &mut state.limits);
        let mut slice = self.options.slice_fuel.min(self.options.total_fuel);
        store
            .set_fuel(slice)
            .map_err(|_| ProbeError::new("runtime_configuration"))?;
        let mut linker = Linker::<State>::new(&engine);
        linker
            .func_wrap(
                "qusor",
                "call",
                |mut caller: Caller<'_, State>,
                 ptr: i32,
                 len: i32,
                 out: i32,
                 cap: i32|
                 -> std::result::Result<i32, wasmi::Error> {
                    match broker_call(&mut caller, ptr, len, out, cap) {
                        Ok(length) => Ok(length),
                        Err(error) => {
                            caller.data_mut().error = Some(error);
                            Err(wasmi::Error::new("host capability call rejected"))
                        }
                    }
                },
            )
            .map_err(|_| ProbeError::new("runtime_configuration"))?;
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|_| ProbeError::new("instantiate_failed"))?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| ProbeError::new("missing_memory"))?;
        let run = instance
            .get_typed_func::<(), (i32, i32)>(&store, "run")
            .map_err(|_| ProbeError::new("invalid_entrypoint"))?;
        self.control.check(started, self.options.deadline)?;
        let mut outcome = run
            .call_resumable(&mut store, ())
            .map_err(|_| vm_error(&store))?;
        let mut spent = 0;
        let (pointer, length) = loop {
            spent += slice
                - store
                    .get_fuel()
                    .map_err(|_| ProbeError::new("runtime_configuration"))?;
            self.control.slices.fetch_add(1, Ordering::SeqCst);
            self.control.check(started, self.options.deadline)?;
            match outcome {
                TypedResumableCall::Finished(result) => break result,
                TypedResumableCall::HostTrap(_) => return Err(vm_error(&store)),
                TypedResumableCall::OutOfFuel(suspended) => {
                    let remaining = self.options.total_fuel - spent;
                    let required = suspended.required_fuel();
                    if remaining == 0 || required > remaining {
                        return Err(ProbeError::new("fuel_exhausted"));
                    }
                    slice = self.options.slice_fuel.max(required).min(remaining);
                    store
                        .set_fuel(slice)
                        .map_err(|_| ProbeError::new("runtime_configuration"))?;
                    outcome = suspended.resume(&mut store).map_err(|_| vm_error(&store))?;
                }
            }
        };
        let authority = self
            .control
            .authority
            .lock()
            .map_err(|_| ProbeError::new("host_state_poisoned"))?;
        self.control
            .check_locked(&authority, started, self.options.deadline)?;
        let pointer = usize::try_from(pointer).map_err(|_| ProbeError::new("invalid_output"))?;
        let length = usize::try_from(length).map_err(|_| ProbeError::new("invalid_output"))?;
        if length > MAX_RESPONSE_BYTES {
            return Err(ProbeError::new("output_size_limit"));
        }
        let mut output = vec![0; length];
        memory
            .read(&store, pointer, &mut output)
            .map_err(|_| ProbeError::new("invalid_output"))?;
        let output = String::from_utf8(output).map_err(|_| ProbeError::new("invalid_output"))?;
        Ok(RunResult {
            output,
            calls: self.control.calls(),
            fuel_consumed: spent,
        })
    }
}

fn vm_error(store: &Store<State>) -> ProbeError {
    store
        .data()
        .error
        .clone()
        .unwrap_or_else(|| ProbeError::new("vm_trap"))
}
fn broker_call(
    caller: &mut Caller<'_, State>,
    ptr: i32,
    len: i32,
    out: i32,
    cap: i32,
) -> Result<i32> {
    let state = caller.data();
    state.control.check(state.started, state.options.deadline)?;
    if state.control.calls.fetch_add(1, Ordering::SeqCst) >= state.options.host_calls {
        return Err(ProbeError::new("host_call_limit"));
    }
    if state.options.block_host_call {
        // Trusted fault injection only. This deliberately demonstrates the limit of fuel.
        std::thread::sleep(Duration::from_secs(60));
    }
    let pointer = usize::try_from(ptr).map_err(|_| ProbeError::new("invalid_pointer"))?;
    let length = usize::try_from(len).map_err(|_| ProbeError::new("invalid_request_size"))?;
    let output_pointer = usize::try_from(out).map_err(|_| ProbeError::new("invalid_pointer"))?;
    let capacity = usize::try_from(cap).map_err(|_| ProbeError::new("invalid_output_capacity"))?;
    if length > extension_api::MAX_REQUEST_BYTES || capacity > MAX_RESPONSE_BYTES {
        return Err(ProbeError::new("host_buffer_limit"));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| ProbeError::new("missing_memory"))?;
    let mut bytes = vec![0; length];
    memory
        .read(&*caller, pointer, &mut bytes)
        .map_err(|_| ProbeError::new("invalid_pointer"))?;
    let request =
        CapabilityRequest::from_slice(&bytes).map_err(|_| ProbeError::new("invalid_request"))?;
    let control = caller.data().control.clone();
    let mut authority = control
        .authority
        .lock()
        .map_err(|_| ProbeError::new("host_state_poisoned"))?;
    control.check_locked(
        &authority,
        caller.data().started,
        caller.data().options.deadline,
    )?;
    let permit = authority
        .authorize(&control.session, request)
        .map_err(policy_error)?;
    // Keep the authority lock through copying bounded bytes to the guest. A revocation
    // therefore occurs either before this read or after it, never halfway through it.
    authority
        .dispatch(permit, |context| {
            if context.request.capability != control.request.capability
                || context.request.resource != control.request.resource
            {
                return Err(extension_api::ApiError::new(
                    ErrorCode::CapabilityUnavailable,
                    "adapter has no response for this exact capability resource",
                ));
            }
            Ok(())
        })
        .map_err(policy_error)?;
    let text = control.response.as_slice();
    if capacity < text.len() {
        return Err(ProbeError::new("output_capacity"));
    }
    memory
        .write(&mut *caller, output_pointer, text)
        .map_err(|_| ProbeError::new("invalid_pointer"))?;
    drop(authority);
    Ok(text.len() as i32)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub ok: bool,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<RunResult>,
}
impl Report {
    pub fn from_result(result: Result<RunResult>) -> Self {
        match result {
            Ok(result) => Self {
                ok: true,
                code: "ok".into(),
                result: Some(result),
            },
            Err(error) => Self::failure(error.code),
        }
    }
    pub fn failure(code: &str) -> Self {
        Self {
            ok: false,
            code: code.into(),
            result: None,
        }
    }
}
