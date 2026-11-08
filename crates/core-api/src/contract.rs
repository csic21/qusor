//! Public, bounded capture wire and immutable raw-text location contract.
//! URI metadata is never fetched. This module has no storage or host authority.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_WIRE_BYTES: usize = 65_536;
pub const PROTOCOL_VERSION: u16 = 1;
pub const TEXT_UNIT: &str = "unicode_scalar";
pub const NORMALIZATION: &str = "qusor-raw-text-v1";
pub type Result<T> = std::result::Result<T, ContractError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractErrorCode {
    Malformed,
    BudgetExceeded,
    UnsupportedVersion,
    InvalidSource,
    InvalidSelector,
    InvalidBoundary,
    QuoteMismatch,
    Ambiguous,
    NotFound,
    VersionMismatch,
    HashMismatch,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractError {
    pub code: ContractErrorCode,
    pub message: &'static str,
}
impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for ContractError {}
fn err(code: ContractErrorCode, message: &'static str) -> ContractError {
    ContractError { code, message }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureRequest {
    pub capture_protocol: u16,
    pub request_id: String,
    pub draft: CaptureDraft,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureResponse {
    pub capture_protocol: u16,
    pub request_id: String,
    pub candidate_id: String,
    pub draft_hash: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureDraft {
    pub protocol_version: u16,
    pub source: SourceDescriptor,
    pub snapshot_text: String,
    pub selector: TextSelector,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDescriptor {
    pub namespace: String,
    pub reference: String,
    pub title: String,
    pub public_uri: Option<String>,
    pub external_return: ExternalReturn,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalReturn {
    None,
    SourceOnly,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextSelector {
    pub unit: String,
    pub normalization: String,
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub exact: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextAnchor {
    pub protocol_version: u16,
    pub snapshot_version: String,
    pub snapshot_sha256: String,
    pub selector: TextSelector,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedRange {
    pub start: u64,
    pub end: u64,
    pub byte_start: u64,
    pub byte_end: u64,
    pub text: String,
}

fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.len() > MAX_WIRE_BYTES {
        return Err(err(
            ContractErrorCode::BudgetExceeded,
            "wire exceeds 64 KiB",
        ));
    }
    serde_json::from_slice(bytes).map_err(|_| {
        err(
            ContractErrorCode::Malformed,
            "invalid or non-unique JSON fields",
        )
    })
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| err(ContractErrorCode::Malformed, "cannot encode JSON"))?;
    if bytes.len() > MAX_WIRE_BYTES {
        return Err(err(
            ContractErrorCode::BudgetExceeded,
            "canonical wire exceeds 64 KiB",
        ));
    }
    Ok(bytes)
}
fn identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}
fn label(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
impl CaptureRequest {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Self = parse(bytes)?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        if self.capture_protocol != PROTOCOL_VERSION {
            return Err(err(
                ContractErrorCode::UnsupportedVersion,
                "unsupported capture protocol",
            ));
        }
        if !identifier(&self.request_id, 128) {
            return Err(err(ContractErrorCode::Malformed, "invalid correlation id"));
        }
        self.draft.validate()?;
        encode(self)?;
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode(self)
    }
}
impl CaptureDraft {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Self = parse(bytes)?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(err(
                ContractErrorCode::UnsupportedVersion,
                "unsupported draft protocol",
            ));
        }
        if !identifier(&self.source.namespace, 128)
            || !label(&self.source.reference, 256)
            || !label(&self.source.title, 512)
        {
            return Err(err(
                ContractErrorCode::InvalidSource,
                "invalid source descriptor",
            ));
        }
        if let Some(uri) = &self.source.public_uri {
            validate_uri(uri)?;
        }
        if self.snapshot_text.len() > MAX_WIRE_BYTES {
            return Err(err(ContractErrorCode::BudgetExceeded, "snapshot too large"));
        }
        self.resolved_selector()?;
        encode(self)?;
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode(self)
    }
    pub fn draft_hash(&self) -> Result<String> {
        Ok(sha256(&self.canonical_bytes()?))
    }
    pub fn snapshot_hash(&self) -> String {
        sha256(self.snapshot_text.as_bytes())
    }
    pub fn resolved_selector(&self) -> Result<ResolvedRange> {
        resolve_selector(&self.snapshot_text, &self.selector)
    }
    pub fn anchor(&self, snapshot_version: &str) -> Result<TextAnchor> {
        self.validate()?;
        if !identifier(snapshot_version, 256) {
            return Err(err(
                ContractErrorCode::VersionMismatch,
                "invalid snapshot version",
            ));
        }
        let range = self.resolved_selector()?;
        let mut selector = self.selector.clone();
        selector.start = Some(range.start);
        selector.end = Some(range.end);
        Ok(TextAnchor {
            protocol_version: PROTOCOL_VERSION,
            snapshot_version: snapshot_version.into(),
            snapshot_sha256: self.snapshot_hash(),
            selector,
        })
    }
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn resolve_anchor(
    text: &str,
    anchor: &TextAnchor,
    expected_version: &str,
) -> Result<ResolvedRange> {
    if anchor.protocol_version != PROTOCOL_VERSION {
        return Err(err(
            ContractErrorCode::UnsupportedVersion,
            "unsupported anchor protocol",
        ));
    }
    if !identifier(&anchor.snapshot_version, 256) || anchor.snapshot_version != expected_version {
        return Err(err(
            ContractErrorCode::VersionMismatch,
            "snapshot version mismatch",
        ));
    }
    if anchor.snapshot_sha256 != sha256(text.as_bytes()) {
        return Err(err(
            ContractErrorCode::HashMismatch,
            "snapshot hash mismatch",
        ));
    }
    resolve_selector(text, &anchor.selector)
}
pub fn resolve_selector(text: &str, selector: &TextSelector) -> Result<ResolvedRange> {
    use ContractErrorCode::*;
    if text.len() > MAX_WIRE_BYTES || selector.exact.len() > MAX_WIRE_BYTES {
        return Err(err(BudgetExceeded, "text exceeds budget"));
    }
    if selector.unit != TEXT_UNIT
        || selector.normalization != NORMALIZATION
        || selector.exact.is_empty()
    {
        return Err(err(InvalidSelector, "invalid text profile or empty quote"));
    }
    let mut offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    offsets.push(text.len());
    let (start, end) = match (selector.start, selector.end) {
        (Some(start), Some(end)) => {
            let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
                return Err(err(InvalidSelector, "range overflow"));
            };
            if start >= end || end >= offsets.len() {
                return Err(err(InvalidSelector, "range outside snapshot"));
            }
            (start, end)
        }
        (None, None) => {
            let mut found = None;
            for (index, byte) in offsets.iter().copied().enumerate() {
                if text[byte..].starts_with(&selector.exact) {
                    if found.is_some() {
                        return Err(err(Ambiguous, "quote has multiple exact matches"));
                    }
                    let byte_end = byte + selector.exact.len();
                    let end = offsets
                        .binary_search(&byte_end)
                        .map_err(|_| err(InvalidBoundary, "quote splits a scalar"))?;
                    found = Some((index, end));
                }
            }
            found.ok_or_else(|| err(NotFound, "quote not found"))?
        }
        _ => return Err(err(InvalidSelector, "range endpoints must be paired")),
    };
    let (byte_start, byte_end) = (offsets[start], offsets[end]);
    let boundary = |byte| byte == text.len() || text.grapheme_indices(true).any(|(i, _)| i == byte);
    if !boundary(byte_start) || !boundary(byte_end) {
        return Err(err(InvalidBoundary, "range splits an extended grapheme"));
    }
    if text[byte_start..byte_end] != selector.exact {
        return Err(err(QuoteMismatch, "range and quote disagree"));
    }
    Ok(ResolvedRange {
        start: start as u64,
        end: end as u64,
        byte_start: byte_start as u64,
        byte_end: byte_end as u64,
        text: selector.exact.clone(),
    })
}

/// Deliberately narrow C0 public-URI profile: HTTP(S), no query, fragment or userinfo.
fn validate_uri(uri: &str) -> Result<()> {
    let invalid = || {
        err(
            ContractErrorCode::InvalidSource,
            "URI must be public HTTP(S) metadata without credentials, query or fragment",
        )
    };
    if uri.len() > 2048
        || !uri.is_ascii()
        || uri
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace() || b"\\?#".contains(&b))
    {
        return Err(invalid());
    }
    let rest = uri
        .strip_prefix("https://")
        .or_else(|| uri.strip_prefix("http://"))
        .ok_or_else(invalid)?;
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() || authority.contains(['@', '%']) {
        return Err(invalid());
    }
    let (host, port) = if let Some(tail) = authority.strip_prefix('[') {
        let (host, suffix) = tail.split_once(']').ok_or_else(invalid)?;
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(invalid());
        }
        (
            host,
            if suffix.is_empty() {
                None
            } else {
                Some(suffix.strip_prefix(':').ok_or_else(invalid)?)
            },
        )
    } else {
        let (host, port) = authority
            .split_once(':')
            .map_or((authority, None), |(h, p)| (h, Some(p)));
        if host.len() > 253
            || host.split('.').any(|part| {
                part.is_empty()
                    || part.len() > 63
                    || part.starts_with('-')
                    || part.ends_with('-')
                    || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(invalid());
        }
        (host, port)
    };
    if host.is_empty()
        || port.is_some_and(|p| {
            p.is_empty()
                || !p.bytes().all(|b| b.is_ascii_digit())
                || p.parse::<u16>().map_or(true, |p| p == 0)
        })
    {
        return Err(invalid());
    }
    let mut bytes = rest.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex = [
                bytes.next().ok_or_else(invalid)?,
                bytes.next().ok_or_else(invalid)?,
            ];
            let hex = std::str::from_utf8(&hex).map_err(|_| invalid())?;
            let decoded = u8::from_str_radix(hex, 16).map_err(|_| invalid())?;
            if decoded.is_ascii_control() || decoded.is_ascii_whitespace() {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
