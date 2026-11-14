//! Bounded, deterministic signed package format for the local B2 experiment.
//!
//! The verifier consumes one immutable byte slice. It never opens package paths,
//! extracts files, executes code, grants capabilities, or trusts a self-claimed key.
use std::{collections::BTreeSet, error::Error, fmt};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use extension_api::{ApiVersion, HOST_API, Manifest, validate_supported_apis};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PACKAGE_FORMAT: u16 = 1;
pub const EXECUTION_PROFILE: &str = "wasm32-unknown-unknown-v1";
pub const VERIFIER_POLICY: u16 = 1;
pub const MAX_PACKAGE_BYTES: usize = 9 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_TOTAL_FILE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FILES: usize = 64;
pub const MAX_PATH_BYTES: usize = 240;
pub const MAX_PATH_DEPTH: usize = 8;
const SIGNING_DOMAIN: &[u8] = b"QUSOR-PACKAGE-PROBE-V1\0";

/// Absolute hard ceilings enforced even when the Host supplies a custom package profile.
pub const HARD_MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
pub const HARD_MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const HARD_MAX_TOTAL_FILE_BYTES: usize = 32 * 1024 * 1024;
pub const HARD_MAX_FILES: usize = 512;

/// Host-injected bounded package size profile for real Rust plugins.
///
/// Legacy [`verify`] and [`build_signed`] continue using [`Self::legacy_v1`] so
/// v1 fixtures remain byte-compatible; Host loaders use [`Self::desktop_standard`]
/// or [`Self::mobile_standard`] to admit MiB-class compiled Wasm modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageBudgets {
    pub max_package_bytes: usize,
    pub max_file_bytes: usize,
    pub max_total_file_bytes: usize,
    pub max_files: usize,
}

impl Default for PackageBudgets {
    fn default() -> Self {
        Self::desktop_standard()
    }
}

impl PackageBudgets {
    /// Legacy v1 probe package budgets (9 MiB encoded, 1 MiB file, 4 MiB total).
    pub const fn legacy_v1() -> Self {
        Self {
            max_package_bytes: MAX_PACKAGE_BYTES,
            max_file_bytes: MAX_FILE_BYTES,
            max_total_file_bytes: MAX_TOTAL_FILE_BYTES,
            max_files: MAX_FILES,
        }
    }

    /// Mobile / Web package budgets aligned with 4 MiB Wasm module limits.
    pub const fn mobile_standard() -> Self {
        Self {
            max_package_bytes: 18 * 1024 * 1024,
            max_file_bytes: 4 * 1024 * 1024,
            max_total_file_bytes: 8 * 1024 * 1024,
            max_files: 128,
        }
    }

    /// Desktop package budgets aligned with 8 MiB Wasm module limits.
    pub const fn desktop_standard() -> Self {
        Self {
            max_package_bytes: 36 * 1024 * 1024,
            max_file_bytes: 8 * 1024 * 1024,
            max_total_file_bytes: 16 * 1024 * 1024,
            max_files: 256,
        }
    }

    /// Fails closed if any budget is zero, inconsistent, or above hard ceilings.
    pub fn validate(&self) -> Result<(), PackageError> {
        if self.max_package_bytes == 0
            || self.max_package_bytes > HARD_MAX_PACKAGE_BYTES
            || self.max_file_bytes == 0
            || self.max_file_bytes > HARD_MAX_FILE_BYTES
            || self.max_total_file_bytes == 0
            || self.max_total_file_bytes > HARD_MAX_TOTAL_FILE_BYTES
            || self.max_files == 0
            || self.max_files > HARD_MAX_FILES
            || self.max_file_bytes > self.max_total_file_bytes
            || self.max_total_file_bytes > self.max_package_bytes
        {
            return Err(fail(
                PackageErrorCode::BudgetExceeded,
                "invalid package budget configuration",
            ));
        }
        Ok(())
    }
}

/// Explicit opt-in switch for loading unsigned packages in `qusor dev`.
///
/// Defaults to `false`. Even when set to `true`, unsigned loading is only
/// compiled and reachable in debug builds (`cfg!(debug_assertions)`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DevUnsignedOptions {
    pub allow_unsigned: bool,
}

/// Whether this build profile allows dev-only unsigned package loading.
pub const fn dev_unsigned_supported() -> bool {
    cfg!(debug_assertions)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageErrorCode {
    Malformed,
    BudgetExceeded,
    InvalidManifest,
    Incompatible,
    UnsafePath,
    FileMismatch,
    UntrustedKey,
    InvalidSignature,
    NonCanonical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageError {
    pub code: PackageErrorCode,
    pub message: &'static str,
}

impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for PackageError {}

fn fail(code: PackageErrorCode, message: &'static str) -> PackageError {
    PackageError { code, message }
}

/// A regular file supplied by the author. There is no link or directory entry.
#[derive(Debug, Clone)]
pub struct PackageFile {
    pub path: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

// Struct field order defines this format's compact canonical JSON order. Do not
// convert an untrusted document through serde_json::Value: that would lose
// duplicate fields before the typed deserializer can reject them.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    payload: Payload,
    signature: SignatureEnvelope,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    package_format: u16,
    manifest: Manifest,
    profile: String,
    entrypoint: String,
    files: Vec<FileRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRecord {
    path: String,
    size: usize,
    mime: String,
    sha256: String,
    data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignatureEnvelope {
    algorithm: String,
    publisher_key: String,
    publisher_fingerprint: String,
    signature: String,
}

/// Only successful verification can construct this value.
///
/// The frozen bytes, digest, publisher key, manifest and decoded module always
/// describe one verified package. Persist raw_bytes(), never reopen the source.
#[derive(Debug)]
pub struct VerifiedPackage {
    manifest: Manifest,
    digest: String,
    publisher_key: String,
    publisher_fingerprint: String,
    entrypoint: String,
    profile: String,
    raw_bytes: Vec<u8>,
    files: Vec<PackageFile>,
    module_index: usize,
    dev_unsigned: bool,
}

impl VerifiedPackage {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn publisher_key(&self) -> &str {
        &self.publisher_key
    }

    pub fn publisher_fingerprint(&self) -> &str {
        &self.publisher_fingerprint
    }

    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub fn module_bytes(&self) -> &[u8] {
        &self.files[self.module_index].bytes
    }

    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn verifier_policy(&self) -> u16 {
        VERIFIER_POLICY
    }

    pub fn is_dev_unsigned(&self) -> bool {
        self.dev_unsigned
    }

    /// Read-only bytes and MIME for an exactly named verified regular file.
    pub fn file(&self, path: &str) -> Option<(&str, &[u8])> {
        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| (file.mime.as_str(), file.bytes.as_slice()))
    }
}

/// Derive the lowercase hexadecimal Ed25519 public key from a 32-byte seed.
///
/// Seed generation and secure key storage are outside this experiment. A fixed
/// test seed must never be used for packages distributed as trusted production.
pub fn publisher_key(signing_seed: &[u8; 32]) -> String {
    encode_hex(
        SigningKey::from_bytes(signing_seed)
            .verifying_key()
            .as_bytes(),
    )
}

/// Deterministically build a canonical package with the same verifier as Host.
///
/// Capability vector and contribution order are preserved as manifest semantics;
/// file records are sorted by their ASCII path. No caller-supplied digest or
/// signature is accepted. This function does not write files or select trust.
pub fn build_signed(
    manifest: Manifest,
    entrypoint: &str,
    files: Vec<PackageFile>,
    signing_seed: &[u8; 32],
) -> Result<Vec<u8>, PackageError> {
    build_signed_for_host(manifest, entrypoint, files, signing_seed, &[HOST_API])
}

/// Explicitly target a trusted host support set without changing legacy defaults.
/// This changes compatibility acceptance, never signed format/domain or grants.
pub fn build_signed_for_host(
    manifest: Manifest,
    entrypoint: &str,
    files: Vec<PackageFile>,
    signing_seed: &[u8; 32],
    supported_apis: &[ApiVersion],
) -> Result<Vec<u8>, PackageError> {
    build_signed_with_budgets(
        manifest,
        entrypoint,
        files,
        signing_seed,
        supported_apis,
        &PackageBudgets::legacy_v1(),
    )
}

/// Build and sign a canonical `.qpk` using host-injected bounded package budgets.
pub fn build_signed_with_budgets(
    manifest: Manifest,
    entrypoint: &str,
    files: Vec<PackageFile>,
    signing_seed: &[u8; 32],
    supported_apis: &[ApiVersion],
    budgets: &PackageBudgets,
) -> Result<Vec<u8>, PackageError> {
    let payload = build_payload(manifest, entrypoint, files, supported_apis, budgets)?;
    let key = SigningKey::from_bytes(signing_seed);
    let publisher_key = encode_hex(key.verifying_key().as_bytes());
    let signature = key.sign(&signing_bytes(&payload, budgets)?);
    let package = Package {
        payload,
        signature: SignatureEnvelope {
            algorithm: "ed25519".to_owned(),
            publisher_key: publisher_key.clone(),
            publisher_fingerprint: digest_hex(key.verifying_key().as_bytes()),
            signature: encode_hex(&signature.to_bytes()),
        },
    };
    let bytes = encode_json(&package, budgets)?;
    // Both author and Host use the identical acceptance boundary.
    verify_with_budgets(&bytes, &publisher_key, supported_apis, budgets)?;
    Ok(bytes)
}

/// Build an unsigned `.qpk` strictly for `qusor dev` local debugging in debug builds.
///
/// This function is compiled out (`#[cfg(debug_assertions)]`) in release builds.
#[cfg(debug_assertions)]
pub fn build_dev_unsigned_with_budgets(
    manifest: Manifest,
    entrypoint: &str,
    files: Vec<PackageFile>,
    supported_apis: &[ApiVersion],
    budgets: &PackageBudgets,
) -> Result<Vec<u8>, PackageError> {
    let payload = build_payload(manifest, entrypoint, files, supported_apis, budgets)?;
    let package = Package {
        payload,
        signature: SignatureEnvelope {
            algorithm: "dev-unsigned".to_owned(),
            publisher_key: "dev-unsigned".to_owned(),
            publisher_fingerprint: "dev-unsigned".to_owned(),
            signature: String::new(),
        },
    };
    let bytes = encode_json(&package, budgets)?;
    verify_dev_unsigned_with_budgets(
        &bytes,
        supported_apis,
        budgets,
        DevUnsignedOptions {
            allow_unsigned: true,
        },
    )?;
    Ok(bytes)
}

fn build_payload(
    manifest: Manifest,
    entrypoint: &str,
    mut files: Vec<PackageFile>,
    supported_apis: &[ApiVersion],
    budgets: &PackageBudgets,
) -> Result<Payload, PackageError> {
    budgets.validate()?;
    check_host_apis(supported_apis)?;
    check_file_budgets(files.iter().map(|file| file.bytes.len()), budgets)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let payload = Payload {
        package_format: PACKAGE_FORMAT,
        manifest,
        profile: EXECUTION_PROFILE.to_owned(),
        entrypoint: entrypoint.to_owned(),
        files: files
            .into_iter()
            .map(|file| FileRecord {
                path: file.path,
                size: file.bytes.len(),
                mime: file.mime,
                sha256: digest_hex(&file.bytes),
                data: encode_hex(&file.bytes),
            })
            .collect(),
    };
    validate_payload(&payload, supported_apis, budgets)?;
    Ok(payload)
}

/// Verify a canonical package against an explicitly supplied trusted public key.
///
/// The Host must select trusted_key from its package-ID-to-key trust record.
/// Matching this key proves possession, not real-world publisher identity.
/// Signature success does not grant capabilities or provide runtime isolation.
pub fn verify(bytes: &[u8], trusted_key: &str) -> Result<VerifiedPackage, PackageError> {
    verify_for_host(bytes, trusted_key, &[HOST_API])
}

/// Verify against an explicit trusted host protocol set. Numeric manifest ranges
/// alone cannot enable unknown protocols or new features in a legacy session.
pub fn verify_for_host(
    bytes: &[u8],
    trusted_key: &str,
    supported_apis: &[ApiVersion],
) -> Result<VerifiedPackage, PackageError> {
    verify_with_budgets(
        bytes,
        trusted_key,
        supported_apis,
        &PackageBudgets::legacy_v1(),
    )
}

/// Verify a canonical package under host-injected bounded package budgets.
pub fn verify_with_budgets(
    bytes: &[u8],
    trusted_key: &str,
    supported_apis: &[ApiVersion],
    budgets: &PackageBudgets,
) -> Result<VerifiedPackage, PackageError> {
    let (package, files, module_index) = decode_canonical_package(bytes, supported_apis, budgets)?;
    if package.signature.algorithm != "ed25519" {
        return Err(fail(
            PackageErrorCode::Incompatible,
            "unsupported signature algorithm",
        ));
    }
    let trusted_bytes: [u8; 32] = decode_hex(trusted_key, 32)?
        .try_into()
        .map_err(|_| fail(PackageErrorCode::UntrustedKey, "invalid trusted public key"))?;
    if package.signature.publisher_key != trusted_key {
        return Err(fail(
            PackageErrorCode::UntrustedKey,
            "publisher key does not match host trust",
        ));
    }
    if package.signature.publisher_fingerprint != digest_hex(&trusted_bytes) {
        return Err(fail(
            PackageErrorCode::UntrustedKey,
            "publisher fingerprint does not match key",
        ));
    }
    let key = VerifyingKey::from_bytes(&trusted_bytes)
        .map_err(|_| fail(PackageErrorCode::UntrustedKey, "invalid trusted public key"))?;
    let signature_bytes: [u8; 64] = decode_hex(&package.signature.signature, 64)?
        .try_into()
        .map_err(|_| {
            fail(
                PackageErrorCode::InvalidSignature,
                "invalid signature encoding",
            )
        })?;
    key.verify_strict(
        &signing_bytes(&package.payload, budgets)?,
        &Signature::from_bytes(&signature_bytes),
    )
    .map_err(|_| {
        fail(
            PackageErrorCode::InvalidSignature,
            "signature verification failed",
        )
    })?;
    Ok(finalize_verified_package(
        package,
        bytes,
        files,
        module_index,
        false,
    ))
}

/// Verify an unsigned local-dev package when explicitly opted in by `qusor dev`.
///
/// Always compiled out in release builds (`#[cfg(debug_assertions)]`), requires
/// `options.allow_unsigned == true`, and still enforces canonical JSON, path
/// safety, Wasm magic/header, file hashes, manifest, and bounded budgets.
#[cfg(debug_assertions)]
pub fn verify_dev_unsigned_with_budgets(
    bytes: &[u8],
    supported_apis: &[ApiVersion],
    budgets: &PackageBudgets,
    options: DevUnsignedOptions,
) -> Result<VerifiedPackage, PackageError> {
    if !options.allow_unsigned {
        return Err(fail(
            PackageErrorCode::UntrustedKey,
            "unsigned package rejected without explicit dev switch",
        ));
    }
    let (package, files, module_index) = decode_canonical_package(bytes, supported_apis, budgets)?;
    if package.signature.algorithm != "dev-unsigned"
        || package.signature.publisher_key != "dev-unsigned"
        || package.signature.publisher_fingerprint != "dev-unsigned"
        || !package.signature.signature.is_empty()
    {
        return Err(fail(
            PackageErrorCode::InvalidSignature,
            "package is not marked as dev-unsigned",
        ));
    }
    Ok(finalize_verified_package(
        package,
        bytes,
        files,
        module_index,
        true,
    ))
}

fn decode_canonical_package(
    bytes: &[u8],
    supported_apis: &[ApiVersion],
    budgets: &PackageBudgets,
) -> Result<(Package, Vec<PackageFile>, usize), PackageError> {
    budgets.validate()?;
    check_host_apis(supported_apis)?;
    if bytes.len() > budgets.max_package_bytes {
        return Err(fail(
            PackageErrorCode::BudgetExceeded,
            "package exceeds encoded byte budget",
        ));
    }
    let package: Package = serde_json::from_slice(bytes).map_err(|_| {
        fail(
            PackageErrorCode::Malformed,
            "invalid package JSON shape or value",
        )
    })?;
    if encode_json(&package, budgets)? != bytes {
        return Err(fail(
            PackageErrorCode::NonCanonical,
            "package is not canonical JSON",
        ));
    }
    let (files, module_index) = validate_payload(&package.payload, supported_apis, budgets)?;
    Ok((package, files, module_index))
}

fn finalize_verified_package(
    package: Package,
    bytes: &[u8],
    files: Vec<PackageFile>,
    module_index: usize,
    dev_unsigned: bool,
) -> VerifiedPackage {
    VerifiedPackage {
        manifest: package.payload.manifest,
        digest: digest_hex(bytes),
        publisher_key: package.signature.publisher_key,
        publisher_fingerprint: package.signature.publisher_fingerprint,
        entrypoint: package.payload.entrypoint,
        profile: package.payload.profile,
        raw_bytes: bytes.to_vec(),
        files,
        module_index,
        dev_unsigned,
    }
}

fn check_host_apis(supported_apis: &[ApiVersion]) -> Result<(), PackageError> {
    validate_supported_apis(supported_apis).map_err(|_| {
        fail(
            PackageErrorCode::Incompatible,
            "invalid trusted host API support set",
        )
    })
}

fn validate_payload(
    payload: &Payload,
    supported_apis: &[ApiVersion],
    budgets: &PackageBudgets,
) -> Result<(Vec<PackageFile>, usize), PackageError> {
    if payload.package_format != PACKAGE_FORMAT || payload.profile != EXECUTION_PROFILE {
        return Err(fail(
            PackageErrorCode::Incompatible,
            "unsupported format or execution profile",
        ));
    }
    // to_json performs structural validation and the manifest's own byte budget.
    payload.manifest.to_json().map_err(|_| {
        fail(
            PackageErrorCode::InvalidManifest,
            "invalid or oversized manifest",
        )
    })?;
    if payload.manifest.negotiate_api(supported_apis).is_none() {
        return Err(fail(
            PackageErrorCode::Incompatible,
            "host API is outside the supported range",
        ));
    }
    // The authoritative API only deserializes known capabilities/contributions;
    // it validates every contribution's declared requirements as well.
    validate_path(&payload.entrypoint)?;
    check_file_budgets(payload.files.iter().map(|file| file.size), budgets)?;
    let mut seen = BTreeSet::new();
    let mut decoded = Vec::with_capacity(payload.files.len());
    let mut previous_path: Option<&str> = None;
    let mut module_index = None;
    for (index, file) in payload.files.iter().enumerate() {
        validate_path(&file.path)?;
        if previous_path.is_some_and(|previous| previous >= file.path.as_str()) {
            return Err(fail(
                PackageErrorCode::UnsafePath,
                "file paths must be unique and sorted",
            ));
        }
        previous_path = Some(&file.path);
        let folded = file.path.to_ascii_lowercase();
        if !seen.insert(folded) {
            return Err(fail(
                PackageErrorCode::UnsafePath,
                "case-insensitive file path collision",
            ));
        }
        validate_mime(&file.mime)?;
        let data = decode_hex(&file.data, file.size)?;
        if file.sha256 != digest_hex(&data) {
            return Err(fail(
                PackageErrorCode::FileMismatch,
                "file digest does not match its bytes",
            ));
        }
        if file.path == payload.entrypoint {
            if file.mime != "application/wasm" || !data.starts_with(b"\0asm\x01\0\0\0") {
                return Err(fail(
                    PackageErrorCode::FileMismatch,
                    "entrypoint is not a version 1 Wasm module",
                ));
            }
            module_index = Some(index);
        }
        decoded.push(PackageFile {
            path: file.path.clone(),
            mime: file.mime.clone(),
            bytes: data,
        });
    }
    // Checking all ancestors after building the complete set catches both order
    // directions, including a directory spelling which differs only in case.
    for path in &seen {
        for (offset, _) in path.match_indices('/') {
            if seen.contains(&path[..offset]) {
                return Err(fail(
                    PackageErrorCode::UnsafePath,
                    "file is also another file's ancestor",
                ));
            }
        }
    }
    let module_index = module_index.ok_or_else(|| {
        fail(
            PackageErrorCode::FileMismatch,
            "entrypoint is absent from the file list",
        )
    })?;
    Ok((decoded, module_index))
}

fn check_file_budgets(
    sizes: impl Iterator<Item = usize>,
    budgets: &PackageBudgets,
) -> Result<(), PackageError> {
    let mut count = 0usize;
    let mut total = 0usize;
    for size in sizes {
        count += 1;
        if count > budgets.max_files || size > budgets.max_file_bytes {
            return Err(fail(
                PackageErrorCode::BudgetExceeded,
                "file count or single-file budget exceeded",
            ));
        }
        total = total
            .checked_add(size)
            .ok_or_else(|| fail(PackageErrorCode::BudgetExceeded, "file total overflow"))?;
        if total > budgets.max_total_file_bytes {
            return Err(fail(
                PackageErrorCode::BudgetExceeded,
                "total decoded file budget exceeded",
            ));
        }
    }
    if count == 0 {
        return Err(fail(PackageErrorCode::FileMismatch, "package has no files"));
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), PackageError> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.split('/').count() > MAX_PATH_DEPTH
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        return Err(fail(
            PackageErrorCode::UnsafePath,
            "invalid or oversized portable file path",
        ));
    }
    for segment in path.split('/') {
        if segment.is_empty() || matches!(segment, "." | "..") || segment.ends_with('.') {
            return Err(fail(
                PackageErrorCode::UnsafePath,
                "empty or ambiguous file path segment",
            ));
        }
        // Windows aliases are rejected on every platform, even though this
        // experiment never extracts entries into a directory tree.
        let base = segment
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let reserved = matches!(base.as_str(), "con" | "prn" | "aux" | "nul")
            || (base.len() == 4
                && (base.starts_with("com") || base.starts_with("lpt"))
                && matches!(base.as_bytes()[3], b'1'..=b'9'));
        if reserved {
            return Err(fail(
                PackageErrorCode::UnsafePath,
                "reserved portable file path segment",
            ));
        }
    }
    Ok(())
}

fn validate_mime(mime: &str) -> Result<(), PackageError> {
    let mut parts = mime.split('/');
    let token = |value: &str| {
        !value.is_empty()
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'+' | b'-')
            })
    };
    if mime.len() > 128
        || !parts.next().is_some_and(token)
        || !parts.next().is_some_and(token)
        || parts.next().is_some()
    {
        return Err(fail(
            PackageErrorCode::FileMismatch,
            "invalid canonical MIME type",
        ));
    }
    Ok(())
}

fn signing_bytes(payload: &Payload, budgets: &PackageBudgets) -> Result<Vec<u8>, PackageError> {
    let encoded = encode_json(payload, budgets)?;
    let mut bytes = Vec::with_capacity(SIGNING_DOMAIN.len() + encoded.len());
    bytes.extend_from_slice(SIGNING_DOMAIN);
    bytes.extend_from_slice(&encoded);
    Ok(bytes)
}

fn encode_json(value: &impl Serialize, budgets: &PackageBudgets) -> Result<Vec<u8>, PackageError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| fail(PackageErrorCode::Malformed, "could not encode package"))?;
    if bytes.len() > budgets.max_package_bytes {
        return Err(fail(
            PackageErrorCode::BudgetExceeded,
            "package exceeds encoded byte budget",
        ));
    }
    Ok(bytes)
}

fn decode_hex(value: &str, expected_bytes: usize) -> Result<Vec<u8>, PackageError> {
    if expected_bytes.checked_mul(2) != Some(value.len()) {
        return Err(fail(
            PackageErrorCode::FileMismatch,
            "hexadecimal byte length mismatch",
        ));
    }
    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = nibble(pair[0]).ok_or_else(|| {
                fail(
                    PackageErrorCode::Malformed,
                    "invalid lowercase hexadecimal encoding",
                )
            })?;
            let low = nibble(pair[1]).ok_or_else(|| {
                fail(
                    PackageErrorCode::Malformed,
                    "invalid lowercase hexadecimal encoding",
                )
            })?;
            Ok(high * 16 + low)
        })
        .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    output
}

fn digest_hex(bytes: &[u8]) -> String {
    encode_hex(&Sha256::digest(bytes))
}
