//! Ordinary signed package and real Core fixture, independent of process execution.
use extension_api::*;
use extension_host::{
    Grant,
    durable::{DurableHost, DurableSession, VersionedRequest},
};
use extension_package::package::{
    PackageFile, VerifiedPackage, build_signed_for_host, publisher_key, verify_for_host,
};
use extension_store::{GrantScope, GrantValidator, Store, StoreError, StoreServices};
use qusor_capture_probe::{
    contract::CaptureDraft,
    core::{Core, Provenance},
};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-supervised-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Policy(Arc<Mutex<DurableHost>>);
impl GrantValidator for Policy {
    fn validate(&self, p: &VerifiedPackage, s: &GrantScope) -> extension_store::Result<()> {
        let api = p
            .manifest()
            .negotiate_api(&SUPPORTED_HOST_APIS)
            .ok_or_else(|| StoreError::invalid("no API"))?;
        self.0
            .lock()
            .unwrap()
            .validate_activation_intent(
                p.manifest(),
                p.digest(),
                &[Grant {
                    capability: s.capability,
                    resource: s.resource(),
                }],
                api,
            )
            .map_err(StoreError::from)
    }
}
pub fn request(api: ApiVersion) -> CapabilityRequest {
    CapabilityRequest {
        api_version: api,
        request_id: "read-real-core".into(),
        capability: Capability::DocumentMetadataRead,
        resource: Resource::Document {
            vault_id: "vault".into(),
            document_id: "1".into(),
        },
    }
}
pub fn signed(id: &str, api: ApiVersion, wasm: Vec<u8>) -> VerifiedPackage {
    let manifest = Manifest {
        manifest_version: 1,
        package_id: id.into(),
        publisher_id: "org.supervised.author".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: api.minor,
            max_minor: api.minor,
        },
        required_capabilities: vec![Capability::DocumentMetadataRead],
        optional_capabilities: vec![],
        contributions: vec![],
    };
    let seed = [52; 32];
    let bytes = build_signed_for_host(
        manifest,
        "module.wasm",
        vec![PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wasm,
        }],
        &seed,
        &SUPPORTED_HOST_APIS,
    )
    .unwrap();
    verify_for_host(&bytes, &publisher_key(&seed), &SUPPORTED_HOST_APIS).unwrap()
}
pub fn module(raw: &[u8], calls: usize) -> Vec<u8> {
    let data = raw.iter().map(|b| format!("\\{b:02x}")).collect::<String>();
    let call = format!(
        "i32.const 0 i32.const {} i32.const 65536 i32.const 4096 call $call local.set $n",
        raw.len()
    );
    wat::parse_str(format!(r#"(module(import "qusor" "call"(func $call(param i32 i32 i32 i32)(result i32)))(memory(export "memory")2 2)(data(i32.const 0)"{data}")(func(export "run")(result i32 i32)(local $n i32){} i32.const 65536 local.get $n))"#,vec![call;calls].join(" "))).unwrap()
}
pub struct World {
    pub scratch: Scratch,
    pub core: Core,
    pub store: Store,
    pub host: Arc<Mutex<DurableHost>>,
    pub version: String,
}
impl World {
    pub fn new() -> Self {
        let scratch = Scratch::new("core");
        let mut core = Core::open(scratch.0.join("knowledge")).unwrap();
        let draft:CaptureDraft=serde_json::from_value(serde_json::json!({"protocol_version":1,"source":{"namespace":"local","reference":"supervised-core-fixture","title":"Actual supervised Core document","public_uri":null,"external_return":"none"},"snapshot_text":"A real immutable local source","selector":{"unit":"unicode_scalar","normalization":"qusor-raw-text-v1","start":0,"end":1,"exact":"A"}})).unwrap();
        let receipt = core
            .commit(
                "fixture-source",
                "vault",
                &draft,
                &Provenance {
                    package_id: "org.supervised.source".into(),
                    publisher_id: "org.supervised.author".into(),
                    package_digest: "b".repeat(64),
                    installation_id: 1,
                },
                "",
                None,
            )
            .unwrap();
        let host = Arc::new(Mutex::new(
            DurableHost::new("vault", [Capability::DocumentMetadataRead]).unwrap(),
        ));
        let store = Store::open_with_services(
            &scratch.0.join("packages"),
            &SUPPORTED_HOST_APIS,
            StoreServices {
                grant_validator: Arc::new(Policy(host.clone())),
                ..StoreServices::default()
            },
        )
        .unwrap();
        Self {
            scratch,
            core,
            store,
            host,
            version: receipt.snapshot_hash,
        }
    }
    pub fn install(&mut self, p: &VerifiedPackage) -> DurableSession {
        let id = &p.manifest().package_id;
        self.store
            .install(&format!("install-{}", id.replace('.', "-")), 0, p)
            .unwrap();
        self.store
            .grant(
                &format!("grant-{}", id.replace('.', "-")),
                1,
                id,
                GrantScope {
                    capability: Capability::DocumentMetadataRead,
                    library: "vault".into(),
                    document: "1".into(),
                    version: self.version.clone(),
                    expires_at: now() + 3600,
                },
            )
            .unwrap();
        self.host
            .lock()
            .unwrap()
            .activate(
                &self.store,
                id,
                p.manifest().negotiate_api(&SUPPORTED_HOST_APIS).unwrap(),
                now(),
            )
            .unwrap()
    }
    pub fn broker(
        &self,
        session: &DurableSession,
        run: &str,
        raw: &[u8],
    ) -> Result<Vec<u8>, ApiError> {
        let request = CapabilityRequest::from_slice(raw)?;
        let record = self
            .core
            .query(1)
            .map_err(|_| ApiError::new(ErrorCode::InvalidState, "Core source unavailable"))?;
        self.host.lock().unwrap().dispatch(&self.store,session,now(),Some(run),VersionedRequest{request,resource_version:Some(record.receipt.snapshot_hash.clone())},|_|serde_json::to_vec(&serde_json::json!({"title":record.draft.source.title,"version":record.receipt.snapshot_hash,"document_version_id":record.receipt.document_version_id,"commit_rev":record.receipt.commit_rev})).map_err(|_|ApiError::new(ErrorCode::InvalidState,"Core metadata encoding failed")))
    }
}
