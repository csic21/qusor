//! Bounded, deterministic signed package format for the local B2 experiment.
//!
//! The verifier consumes one immutable byte slice. It never opens package paths,
//! extracts files, executes code, grants capabilities, or trusts a self-claimed key.
use std::{collections::BTreeSet, error::Error, fmt};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use extension_api::{HOST_API, Manifest};
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
    mut files: Vec<PackageFile>,
    signing_seed: &[u8; 32],
) -> Result<Vec<u8>, PackageError> {
    check_file_budgets(files.iter().map(|file| file.bytes.len()))?;
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
    validate_payload(&payload)?;
    let key = SigningKey::from_bytes(signing_seed);
    let publisher_key = encode_hex(key.verifying_key().as_bytes());
    let signature = key.sign(&signing_bytes(&payload)?);
    let package = Package {
        payload,
        signature: SignatureEnvelope {
            algorithm: "ed25519".to_owned(),
            publisher_key: publisher_key.clone(),
            publisher_fingerprint: digest_hex(key.verifying_key().as_bytes()),
            signature: encode_hex(&signature.to_bytes()),
        },
    };
    let bytes = encode_json(&package)?;
    // Both author and Host use the identical acceptance boundary.
    verify(&bytes, &publisher_key)?;
    Ok(bytes)
}

/// Verify a canonical package against an explicitly supplied trusted public key.
///
/// The Host must select trusted_key from its package-ID-to-key trust record.
/// Matching this key proves possession, not real-world publisher identity.
/// Signature success does not grant capabilities or provide runtime isolation.
pub fn verify(bytes: &[u8], trusted_key: &str) -> Result<VerifiedPackage, PackageError> {
    if bytes.len() > MAX_PACKAGE_BYTES {
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
    if encode_json(&package)? != bytes {
        return Err(fail(
            PackageErrorCode::NonCanonical,
            "package is not canonical JSON",
        ));
    }
    let (files, module_index) = validate_payload(&package.payload)?;
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
        &signing_bytes(&package.payload)?,
        &Signature::from_bytes(&signature_bytes),
    )
    .map_err(|_| {
        fail(
            PackageErrorCode::InvalidSignature,
            "signature verification failed",
        )
    })?;
    Ok(VerifiedPackage {
        manifest: package.payload.manifest,
        digest: digest_hex(bytes),
        publisher_key: package.signature.publisher_key,
        publisher_fingerprint: package.signature.publisher_fingerprint,
        entrypoint: package.payload.entrypoint,
        profile: package.payload.profile,
        raw_bytes: bytes.to_vec(),
        files,
        module_index,
    })
}

fn validate_payload(payload: &Payload) -> Result<(Vec<PackageFile>, usize), PackageError> {
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
    if !payload.manifest.host_api.supports(HOST_API) {
        return Err(fail(
            PackageErrorCode::Incompatible,
            "host API is outside the supported range",
        ));
    }
    // The authoritative API only deserializes known capabilities/contributions;
    // it validates every contribution's declared requirements as well.
    validate_path(&payload.entrypoint)?;
    check_file_budgets(payload.files.iter().map(|file| file.size))?;
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

fn check_file_budgets(sizes: impl Iterator<Item = usize>) -> Result<(), PackageError> {
    let mut count = 0usize;
    let mut total = 0usize;
    for size in sizes {
        count += 1;
        if count > MAX_FILES || size > MAX_FILE_BYTES {
            return Err(fail(
                PackageErrorCode::BudgetExceeded,
                "file count or single-file budget exceeded",
            ));
        }
        total = total
            .checked_add(size)
            .ok_or_else(|| fail(PackageErrorCode::BudgetExceeded, "file total overflow"))?;
        if total > MAX_TOTAL_FILE_BYTES {
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

fn signing_bytes(payload: &Payload) -> Result<Vec<u8>, PackageError> {
    let encoded = encode_json(payload)?;
    let mut bytes = Vec::with_capacity(SIGNING_DOMAIN.len() + encoded.len());
    bytes.extend_from_slice(SIGNING_DOMAIN);
    bytes.extend_from_slice(&encoded);
    Ok(bytes)
}

fn encode_json(value: &impl Serialize) -> Result<Vec<u8>, PackageError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| fail(PackageErrorCode::Malformed, "could not encode package"))?;
    if bytes.len() > MAX_PACKAGE_BYTES {
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
