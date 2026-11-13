//! Immutable EPUB chapter and table-of-contents contract.
//! No parser, filesystem, SQL, runtime, or UI implementation belongs here.
use crate::contract::sha256;
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

/// Protocol version for EPUB anchor drafts and capture selections.
pub const EPUB_PROTOCOL_VERSION: u16 = 1;
/// Canonical text unit for EPUB scalar range selectors.
pub const EPUB_TEXT_UNIT: &str = "unicode_scalar";
/// Canonical normalization tag for extracted EPUB chapter text.
pub const EPUB_NORMALIZATION: &str = "qusor-epub-text-v1";
/// Maximum number of chapters allowed in one EPUB snapshot.
pub const MAX_EPUB_CHAPTERS: usize = 500;
/// Maximum UTF-8 byte length of a single EPUB chapter's extracted text (2 MiB).
pub const MAX_EPUB_CHAPTER_BYTES: usize = 2 * 1024 * 1024;
/// Maximum number of TOC entries allowed in one EPUB snapshot.
pub const MAX_EPUB_TOC_ENTRIES: usize = 1024;
/// Maximum original EPUB archive byte size (16 MiB).
pub const MAX_EPUB_BYTES: usize = 16 * 1024 * 1024;
/// Maximum UTF-8 bytes of a vault identifier.
pub const MAX_EPUB_VAULT_BYTES: usize = 256;
/// Maximum UTF-8 bytes of an EPUB or chapter/TOC title.
pub const MAX_EPUB_TITLE_BYTES: usize = 1024;
/// Maximum UTF-8 bytes of an EPUB author field.
pub const MAX_EPUB_AUTHOR_BYTES: usize = 512;
/// Maximum UTF-8 bytes of a chapter href.
pub const MAX_EPUB_HREF_BYTES: usize = 1024;
/// Maximum UTF-8 bytes of an extraction revision string.
pub const MAX_EPUB_REVISION_BYTES: usize = 128;
/// Maximum nesting depth of a TOC entry.
pub const MAX_EPUB_TOC_DEPTH: u8 = 16;
/// Maximum encoded JSON size of an EPUB snapshot (32 MiB).
pub const MAX_EPUB_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
/// Maximum encoded JSON size of an EPUB draft request (64 KiB).
pub const MAX_EPUB_REQUEST_BYTES: usize = 65_536;

pub type Result<T> = std::result::Result<T, EpubContractError>;

/// Error categories for EPUB contract validation and anchor resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpubContractErrorCode {
    Malformed,
    BudgetExceeded,
    UnsupportedVersion,
    InvalidIdentity,
    InvalidSelector,
    InvalidBoundary,
    QuoteMismatch,
    HashMismatch,
    OriginalMismatch,
    NoText,
}

/// Typed EPUB contract error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubContractError {
    pub code: EpubContractErrorCode,
    pub message: &'static str,
}

impl std::fmt::Display for EpubContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for EpubContractError {}

fn err(code: EpubContractErrorCode, message: &'static str) -> EpubContractError {
    EpubContractError { code, message }
}

fn digest_ok(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn label_ok(value: &str, max: usize, required: bool) -> bool {
    if required && value.is_empty() {
        return false;
    }
    value.len() <= max && !value.chars().any(char::is_control)
}

fn chapter_text_ok(value: &str, max: usize) -> bool {
    value.len() <= max
        && !value
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
}

fn encode<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| err(EpubContractErrorCode::Malformed, "cannot encode EPUB JSON"))?;
    if bytes.len() > limit {
        return Err(err(
            EpubContractErrorCode::BudgetExceeded,
            "EPUB JSON exceeds budget",
        ));
    }
    Ok(bytes)
}

fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T> {
    if bytes.len() > limit {
        return Err(err(
            EpubContractErrorCode::BudgetExceeded,
            "EPUB JSON exceeds budget",
        ));
    }
    serde_json::from_slice(bytes).map_err(|_| {
        err(
            EpubContractErrorCode::Malformed,
            "invalid, unknown or duplicate EPUB JSON fields",
        )
    })
}

/// Extracted immutable text snapshot of one EPUB spine chapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubChapterSnapshot {
    pub chapter_index: u32,
    pub title: String,
    pub href: String,
    pub text: String,
}

/// One bounded entry in an EPUB table of contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubTocEntry {
    pub title: String,
    pub chapter_index: u32,
    pub depth: u8,
}

/// Complete immutable EPUB extraction snapshot owned by Core upon import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubSnapshot {
    pub vault: String,
    pub title: String,
    pub author: String,
    pub original_sha256: String,
    pub original_bytes: u64,
    pub extraction_revision: String,
    pub chapters: Vec<EpubChapterSnapshot>,
    pub toc: Vec<EpubTocEntry>,
}

/// Uncommitted or saved EPUB scalar-range anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubDraft {
    pub protocol_version: u16,
    pub original_sha256: String,
    pub extraction_revision: String,
    pub chapter_index: u32,
    pub chapter_text_sha256: String,
    pub unit: String,
    pub start: u64,
    pub end: u64,
    pub exact: String,
}

/// An anchor is a fully constrained immutable draft, never an inferred remapping.
pub type EpubAnchor = EpubDraft;

/// Resolved scalar, UTF-8 byte, and UTF-16 code unit selection inside one chapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubSelection {
    pub anchor: EpubAnchor,
    pub chapter_index: u32,
    pub exact: String,
    pub start: u64,
    pub end: u64,
    pub byte_start: u64,
    pub byte_end: u64,
    pub utf16_start: u64,
    pub utf16_end: u64,
}

impl EpubSnapshot {
    /// Deserialize and validate an EPUB snapshot from JSON bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Self = parse(bytes, MAX_EPUB_SNAPSHOT_BYTES)?;
        value.validate()?;
        Ok(value)
    }

    /// Validate all bounds, digests, chapter ordering, and TOC targets.
    pub fn validate(&self) -> Result<()> {
        use EpubContractErrorCode::*;
        if !label_ok(&self.vault, MAX_EPUB_VAULT_BYTES, true)
            || !label_ok(&self.title, MAX_EPUB_TITLE_BYTES, true)
            || !label_ok(&self.author, MAX_EPUB_AUTHOR_BYTES, false)
            || !label_ok(&self.extraction_revision, MAX_EPUB_REVISION_BYTES, true)
        {
            return Err(err(InvalidIdentity, "invalid EPUB metadata"));
        }
        if !digest_ok(&self.original_sha256) {
            return Err(err(HashMismatch, "invalid EPUB original digest"));
        }
        if self.original_bytes == 0 || self.original_bytes > MAX_EPUB_BYTES as u64 {
            return Err(err(BudgetExceeded, "EPUB original byte budget invalid"));
        }
        if self.chapters.is_empty() || self.chapters.len() > MAX_EPUB_CHAPTERS {
            return Err(err(BudgetExceeded, "EPUB chapter count outside budget"));
        }
        if self.toc.len() > MAX_EPUB_TOC_ENTRIES {
            return Err(err(BudgetExceeded, "EPUB TOC entry count exceeds budget"));
        }
        for (idx, chapter) in self.chapters.iter().enumerate() {
            if chapter.chapter_index as usize != idx {
                return Err(err(Malformed, "non-contiguous EPUB chapter index"));
            }
            if !label_ok(&chapter.title, MAX_EPUB_TITLE_BYTES, false)
                || !label_ok(&chapter.href, MAX_EPUB_HREF_BYTES, true)
            {
                return Err(err(InvalidIdentity, "invalid EPUB chapter metadata"));
            }
            if chapter.text.len() > MAX_EPUB_CHAPTER_BYTES {
                return Err(err(BudgetExceeded, "EPUB chapter text exceeds 2 MiB"));
            }
            if !chapter_text_ok(&chapter.text, MAX_EPUB_CHAPTER_BYTES) {
                return Err(err(
                    Malformed,
                    "EPUB chapter text contains unsupported control characters",
                ));
            }
        }
        for entry in &self.toc {
            if !label_ok(&entry.title, MAX_EPUB_TITLE_BYTES, true) {
                return Err(err(InvalidIdentity, "invalid EPUB TOC title"));
            }
            if (entry.chapter_index as usize) >= self.chapters.len() {
                return Err(err(Malformed, "EPUB TOC points to unknown chapter"));
            }
            if entry.depth > MAX_EPUB_TOC_DEPTH {
                return Err(err(BudgetExceeded, "EPUB TOC depth exceeds maximum"));
            }
        }
        encode(self, MAX_EPUB_SNAPSHOT_BYTES)?;
        Ok(())
    }

    /// Verify raw original EPUB bytes match this snapshot's size and SHA-256 digest.
    pub fn validate_original(&self, original: &[u8]) -> Result<()> {
        self.validate()?;
        if original.len() > MAX_EPUB_BYTES {
            return Err(err(
                EpubContractErrorCode::BudgetExceeded,
                "original EPUB exceeds budget",
            ));
        }
        if original.len() as u64 != self.original_bytes || sha256(original) != self.original_sha256
        {
            return Err(err(
                EpubContractErrorCode::OriginalMismatch,
                "original EPUB bytes do not match immutable snapshot",
            ));
        }
        Ok(())
    }

    /// Deterministic SHA-256 hash of the validated semantic EPUB snapshot.
    pub fn snapshot_hash(&self) -> Result<String> {
        self.validate()?;
        Ok(sha256(&encode(self, MAX_EPUB_SNAPSHOT_BYTES)?))
    }

    /// Resolve a scalar-range draft against this immutable snapshot.
    pub fn resolve(&self, draft: &EpubDraft) -> Result<EpubSelection> {
        resolve_epub(self, draft)
    }
}

impl EpubDraft {
    /// Deserialize and validate an EPUB anchor draft from JSON bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Self = parse(bytes, MAX_EPUB_REQUEST_BYTES)?;
        value.validate()?;
        Ok(value)
    }

    /// Validate protocol, digests, unit, and scalar range shape.
    pub fn validate(&self) -> Result<()> {
        use EpubContractErrorCode::*;
        if self.protocol_version != EPUB_PROTOCOL_VERSION {
            return Err(err(UnsupportedVersion, "unsupported EPUB protocol"));
        }
        if !digest_ok(&self.original_sha256) || !digest_ok(&self.chapter_text_sha256) {
            return Err(err(HashMismatch, "invalid EPUB anchor digest"));
        }
        if !label_ok(&self.extraction_revision, MAX_EPUB_REVISION_BYTES, true) {
            return Err(err(InvalidIdentity, "invalid EPUB extraction revision"));
        }
        if self.chapter_index as usize >= MAX_EPUB_CHAPTERS
            || self.unit != EPUB_TEXT_UNIT
            || self.start >= self.end
            || self.exact.is_empty()
        {
            return Err(err(InvalidSelector, "invalid EPUB scalar selector"));
        }
        encode(self, MAX_EPUB_REQUEST_BYTES)?;
        Ok(())
    }

    /// Canonical JSON encoding of this draft.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode(self, MAX_EPUB_REQUEST_BYTES)
    }

    /// Deterministic SHA-256 digest of the canonical draft bytes.
    pub fn draft_hash(&self) -> Result<String> {
        Ok(sha256(&self.canonical_bytes()?))
    }
}

/// Resolve an exact immutable EPUB scalar range; never searches, reorders, or splits graphemes.
pub fn resolve_epub(snapshot: &EpubSnapshot, draft: &EpubDraft) -> Result<EpubSelection> {
    use EpubContractErrorCode::*;
    snapshot.validate()?;
    draft.validate()?;
    if draft.original_sha256 != snapshot.original_sha256
        || draft.extraction_revision != snapshot.extraction_revision
    {
        return Err(err(HashMismatch, "original/extraction revision mismatch"));
    }
    let chapter = snapshot
        .chapters
        .get(draft.chapter_index as usize)
        .ok_or_else(|| err(InvalidSelector, "chapter not in snapshot"))?;
    if chapter.text.is_empty() {
        return Err(err(NoText, "chapter has no text"));
    }
    if draft.chapter_text_sha256 != sha256(chapter.text.as_bytes()) {
        return Err(err(HashMismatch, "anchor chapter hash mismatch"));
    }
    let Ok(start) = usize::try_from(draft.start) else {
        return Err(err(InvalidSelector, "range start overflow"));
    };
    let Ok(end) = usize::try_from(draft.end) else {
        return Err(err(InvalidSelector, "range end overflow"));
    };
    let mut byte_start = None;
    let mut byte_end = None;
    let mut utf16_start = 0u64;
    let mut utf16_end = 0u64;
    let mut utf16_cursor = 0u64;
    let mut scalar_count = 0usize;
    for (byte_idx, ch) in chapter.text.char_indices() {
        if scalar_count == start {
            byte_start = Some(byte_idx);
            utf16_start = utf16_cursor;
        }
        if scalar_count == end {
            byte_end = Some(byte_idx);
            utf16_end = utf16_cursor;
            break;
        }
        utf16_cursor += ch.len_utf16() as u64;
        scalar_count += 1;
    }
    if scalar_count == end && byte_end.is_none() {
        byte_end = Some(chapter.text.len());
        utf16_end = utf16_cursor;
    }
    let (Some(byte_start), Some(byte_end)) = (byte_start, byte_end) else {
        return Err(err(InvalidSelector, "range outside chapter"));
    };
    let boundary = |byte| {
        byte == chapter.text.len()
            || chapter
                .text
                .grapheme_indices(true)
                .any(|(idx, _)| idx == byte)
    };
    if !boundary(byte_start) || !boundary(byte_end) {
        return Err(err(
            InvalidBoundary,
            "EPUB selection splits an extended grapheme",
        ));
    }
    if chapter.text[byte_start..byte_end] != draft.exact {
        return Err(err(QuoteMismatch, "EPUB range and exact quote disagree"));
    }
    Ok(EpubSelection {
        anchor: draft.clone(),
        chapter_index: draft.chapter_index,
        exact: draft.exact.clone(),
        start: draft.start,
        end: draft.end,
        byte_start: byte_start as u64,
        byte_end: byte_end as u64,
        utf16_start,
        utf16_end,
    })
}
