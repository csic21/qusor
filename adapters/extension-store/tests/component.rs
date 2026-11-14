// The durable store admits only a qualified native filesystem profile: the 0700
// directory, no-follow package reads and single-writer lease are implemented for
// Unix. Windows private-storage ACLs and the SQLite/blob adapters are a recorded
// gap (docs/product/release-and-update.md), so these real-store scenarios do not
// run there yet. tests/platform_profile.rs pins the deliberate refusal instead.
#![cfg(unix)]

use extension_api::{HOST_API_V0_3, Manifest};
use extension_package::package::{
    PackageFile, VerifiedPackage, build_signed_for_host, publisher_key, verify_for_host,
};
use extension_ports::{AuthorityRead, ComponentAuthoritySource};
use extension_store::Store;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "qusor-component-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn package(asset: bool) -> VerifiedPackage {
    let manifest = Manifest::from_json(r#"{"manifest_version":1,"package_id":"ordinary.editor","publisher_id":"ordinary.author","package_version":{"major":1,"minor":0,"patch":0},"host_api":{"major":0,"min_minor":3,"max_minor":3},"required_capabilities":["note.draft.edit"],"optional_capabilities":[],"contributions":[{"id":"ordinary.editor.input","title":"Thought editor","kind":"component_provider","required_capabilities":["note.draft.edit"]}]}"#).unwrap();
    let mut files = vec![PackageFile {
        path: "module.wasm".into(),
        mime: "application/wasm".into(),
        bytes: b"\0asm\x01\0\0\0".to_vec(),
    }];
    if asset {
        files.push(PackageFile {path:"assets/ui-composition.json".into(),mime:"application/json".into(),bytes:br#"{"composition_protocol":1,"api_version":{"major":0,"minor":3},"contribution_id":"ordinary.editor.input","role":"annotation.plain-text-editor","contract":"annotation.plain-text-editor/v1","view":{"kind":"text_editor","label":"My thought","placeholder":"Write here"}}"#.to_vec()});
    }
    let bytes =
        build_signed_for_host(manifest, "module.wasm", files, &[44; 32], &[HOST_API_V0_3]).unwrap();
    verify_for_host(&bytes, &publisher_key(&[44; 32]), &[HOST_API_V0_3]).unwrap()
}
#[test]
fn vault_affinity_reopens_without_any_persistent_draft_grant() {
    let scratch = Scratch::new();
    let path = scratch.0.join("store");
    let mut store = Store::open_for_host(&path, &[HOST_API_V0_3]).unwrap();
    let p = package(true);
    let installed = store.install("install", 0, &p).unwrap();
    assert!(
        store
            .component_installation("ordinary.editor", AuthorityRead::Activation)
            .is_err()
    );
    let bound = store
        .bind_component_vault("bind", installed.revision, "ordinary.editor", "vault")
        .unwrap();
    assert_eq!(bound.status, "disabled");
    assert!(store.saved_grant(&bound).is_err());
    let facts = store
        .component_installation("ordinary.editor", AuthorityRead::Activation)
        .unwrap();
    assert_eq!(facts.vault_id, "vault");
    assert_eq!(facts.digest, p.digest());
    assert_eq!(facts.generation, bound.generation);
    assert_eq!(facts.composition_sha256.len(), 64);
    assert_eq!(
        store
            .bind_component_vault("bind", installed.revision, "ordinary.editor", "vault")
            .unwrap(),
        bound
    );
    assert!(
        store
            .bind_component_vault("wrong", bound.revision, "ordinary.editor", "another")
            .is_err()
    );
    drop(store);
    let mut store = Store::open_for_host(&path, &[HOST_API_V0_3]).unwrap();
    let reopened = store
        .component_installation("ordinary.editor", AuthorityRead::Current { run_id: None })
        .unwrap();
    assert_eq!(reopened.generation, bound.generation);
    let stopped = store
        .stop("stop", bound.revision, "ordinary.editor", "disable")
        .unwrap();
    assert!(stopped.generation > reopened.generation);
    let stopped_facts = store
        .component_installation("ordinary.editor", AuthorityRead::Current { run_id: None })
        .unwrap();
    assert_eq!(stopped_facts.generation, stopped.generation);
    // These remain facts only. A new live user mount is still necessary after stop.
    store
        .uninstall("uninstall", stopped.revision, "ordinary.editor")
        .unwrap();
    assert!(
        store
            .component_installation("ordinary.editor", AuthorityRead::Activation)
            .is_err()
    );
}
#[test]
fn missing_signed_descriptor_cannot_bind_or_advance_installation() {
    let scratch = Scratch::new();
    let mut store = Store::open_for_host(&scratch.0.join("store"), &[HOST_API_V0_3]).unwrap();
    let p = package(false);
    let installed = store.install("install", 0, &p).unwrap();
    assert!(
        store
            .bind_component_vault("bind", installed.revision, "ordinary.editor", "vault")
            .is_err()
    );
    assert_eq!(
        store.current("ordinary.editor").unwrap().revision,
        installed.revision
    );
}

#[test]
fn component_store_stop_advances_generation_without_persisting_draft_grant() {
    let scratch = Scratch::new();
    let mut store = Store::open_for_host(&scratch.0.join("store"), &[HOST_API_V0_3]).unwrap();
    let installed = store.install("install", 0, &package(true)).unwrap();
    let bound = store
        .bind_component_vault("bind", installed.revision, "ordinary.editor", "vault")
        .unwrap();
    let facts = store
        .component_installation("ordinary.editor", AuthorityRead::Activation)
        .unwrap();
    assert_eq!(facts.generation, bound.generation);
    let stopped = store
        .stop("stop", bound.revision, "ordinary.editor", "disable")
        .unwrap();
    assert!(stopped.generation > bound.generation);
    let after = store
        .component_installation("ordinary.editor", AuthorityRead::Current { run_id: None })
        .unwrap();
    assert_eq!(after.generation, stopped.generation);
    // No saved grant can silently reconstruct a component session after the stop.
    assert!(
        store
            .saved_grant(&store.current("ordinary.editor").unwrap())
            .is_err()
    );
}
