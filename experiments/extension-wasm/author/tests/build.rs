use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use sha2::{Digest, Sha256};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("qusor-wasm-author-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn input(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, content).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const MODULE: &str = r#"(module
  (memory (export "memory") 1 2)
  (func (export "run") (result i32 i32) i32.const 0 i32.const 0))"#;

#[test]
fn command_builds_reproducible_module_and_digest_descriptor() {
    let scratch = Scratch::new();
    let source = scratch.input("input.wat", MODULE);
    let manifest = scratch.input("manifest.json", "{\"manifest_version\":1}\n");
    let output = scratch.0.join("package");
    let result = Command::new(env!("CARGO_BIN_EXE_build-plugin"))
        .args([&source, &manifest, &output])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let module = fs::read(output.join("module.wasm")).unwrap();
    assert_eq!(&module[..8], b"\0asm\x01\0\0\0");
    let package = fs::read(output.join("package.json")).unwrap();
    let descriptor: serde_json::Value = serde_json::from_slice(&package).unwrap();
    assert_eq!(descriptor["format_version"], 1);
    assert_eq!(descriptor["entry"], "module.wasm");
    assert_eq!(
        descriptor["sha256"],
        format!("{:x}", Sha256::digest(&module))
    );
    assert_eq!(
        fs::read(output.join("manifest.json")).unwrap(),
        fs::read(&manifest).unwrap()
    );
    let repeat = Command::new(env!("CARGO_BIN_EXE_build-plugin"))
        .args([&source, &manifest, &output])
        .output()
        .unwrap();
    assert!(repeat.status.success());
    assert_eq!(fs::read(output.join("module.wasm")).unwrap(), module);
    assert_eq!(fs::read(output.join("package.json")).unwrap(), package);
}

#[test]
fn builder_does_not_normalize_duplicate_manifest_fields_before_host_validation() {
    let scratch = Scratch::new();
    let source = scratch.input("input.wat", MODULE);
    let bytes = "{\"manifest_version\":2,\"manifest_version\":1}";
    let manifest = scratch.input("manifest.json", bytes);
    let output = scratch.0.join("package");
    let result = Command::new(env!("CARGO_BIN_EXE_build-plugin"))
        .args([source, manifest, output.clone()])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        fs::read_to_string(output.join("manifest.json")).unwrap(),
        bytes
    );
}

#[test]
fn parse_failures_do_not_create_output_packages() {
    for (wat, json) in [("(module broken)", "{}"), (MODULE, "{broken}")] {
        let scratch = Scratch::new();
        let source = scratch.input("input.wat", wat);
        let manifest = scratch.input("manifest.json", json);
        let output = scratch.0.join("package");
        let result = Command::new(env!("CARGO_BIN_EXE_build-plugin"))
            .args([source, manifest, output.clone()])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!output.exists());
    }
}

#[test]
fn command_requires_exactly_three_arguments() {
    for args in [vec![], vec!["one"], vec!["one", "two", "three", "four"]] {
        let result = Command::new(env!("CARGO_BIN_EXE_build-plugin"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("usage:"));
    }
}
