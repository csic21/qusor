//! Persistent knowledge wire envelopes, pure input guards, and anchor helpers.
//! Full cross-entity, outbox, index-job, and commit-log verification lives in
//! `learning_ports::archive`. No filesystem, database, runtime, or UI code belongs here.
use crate::contract::{CaptureDraft, sha256};
use crate::epub_contract::{EpubChapterSnapshot, EpubDraft, EpubSnapshot, EpubTocEntry};
pub use crate::error::{KnowledgeError, KnowledgeErrorCode, KnowledgeResult as Result};
use crate::error::{is_bounded_label, is_bounded_text, is_sha256_hex};
use crate::pdf_compound::PdfCompoundDraft;
use crate::pdf_contract::{PdfDraft, PdfSnapshot};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const LIMIT: usize = 64 * 1024 * 1024;
pub const ARCHIVE_LIMIT: usize = LIMIT;
pub const MAX_ORIGINAL: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub package_id: String,
    pub publisher_id: String,
    pub package_digest: String,
    pub installation_id: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub operation_id: String,
    pub capture_id: i64,
    pub source_id: i64,
    pub document_version_id: i64,
    pub commit_rev: i64,
    pub snapshot_hash: String,
    pub draft_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CaptureRecord {
    pub receipt: Receipt,
    pub vault: String,
    pub draft: CaptureDraft,
    pub provenance: Provenance,
    pub user_note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExportEntry {
    pub record: CaptureRecord,
    pub input_hash: String,
    pub operation: String,
    pub outbox: String,
    pub index_job: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Blob {
    pub hash: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExportBundle {
    pub schema_version: u32,
    pub entries: Vec<ExportEntry>,
    pub blobs: Vec<Blob>,
}

impl ExportBundle {
    /// Validate wire schema version and bounded export payload size.
    pub fn validate_envelope(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(KnowledgeError::unsupported_version(
                "unsupported text archive schema",
            ));
        }
        if self.entries.len() > 4096 || encode_json_bytes(self)?.len() > LIMIT {
            return Err(KnowledgeError::budget_exceeded(
                "text export budget exceeded",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreReceipt {
    pub restored: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfDocumentReceipt {
    pub operation_id: String,
    pub document_id: i64,
    pub commit_rev: i64,
    pub original_sha256: String,
    pub extraction_revision: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfDocumentRecord {
    pub receipt: PdfDocumentReceipt,
    pub vault: String,
    pub title: String,
    pub snapshot: PdfSnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfReceipt {
    pub operation_id: String,
    pub capture_id: i64,
    pub document_id: i64,
    pub commit_rev: i64,
    pub original_sha256: String,
    pub extraction_revision: String,
    pub draft_hash: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfCaptureRecord {
    pub receipt: PdfReceipt,
    pub draft: PdfDraft,
    /// Present only for an indivisible ordered multi-segment selection. `draft`
    /// is its first-segment compatibility projection, never a complete anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compound: Option<PdfCompoundDraft>,
    pub provenance: Provenance,
    pub user_note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubDocumentReceipt {
    pub operation_id: String,
    pub document_id: i64,
    pub commit_rev: i64,
    pub original_sha256: String,
    pub extraction_revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubChapterRecord {
    pub chapter_index: u32,
    pub title: String,
    pub href: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubTocRecord {
    pub title: String,
    pub chapter_index: u32,
    pub depth: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubDocumentRecord {
    pub receipt: EpubDocumentReceipt,
    pub vault: String,
    pub title: String,
    pub author: String,
    pub original_bytes: u64,
    pub chapters: Vec<EpubChapterRecord>,
    pub toc: Vec<EpubTocRecord>,
}

impl EpubDocumentRecord {
    /// Convert this stored record into a validated semantic `EpubSnapshot`.
    pub fn to_snapshot(&self) -> EpubSnapshot {
        EpubSnapshot {
            vault: self.vault.clone(),
            title: self.title.clone(),
            author: self.author.clone(),
            original_sha256: self.receipt.original_sha256.clone(),
            original_bytes: self.original_bytes,
            extraction_revision: self.receipt.extraction_revision.clone(),
            chapters: self
                .chapters
                .iter()
                .map(|c| EpubChapterSnapshot {
                    chapter_index: c.chapter_index,
                    title: c.title.clone(),
                    href: c.href.clone(),
                    text: c.text.clone(),
                })
                .collect(),
            toc: self
                .toc
                .iter()
                .map(|t| EpubTocEntry {
                    title: t.title.clone(),
                    chapter_index: t.chapter_index,
                    depth: t.depth,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubReceipt {
    pub operation_id: String,
    pub capture_id: i64,
    pub document_id: i64,
    pub commit_rev: i64,
    pub original_sha256: String,
    pub extraction_revision: String,
    pub draft_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubCaptureRecord {
    pub receipt: EpubReceipt,
    pub draft: EpubDraft,
    pub provenance: Provenance,
    pub user_note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BinaryBlob {
    pub hash: String,
    pub byte_len: u64,
    pub base64: String,
}

impl BinaryBlob {
    pub fn new(bytes: &[u8]) -> Self {
        Self {
            hash: sha256(bytes),
            byte_len: bytes.len() as u64,
            base64: STANDARD.encode(bytes),
        }
    }

    pub fn decode(&self) -> Result<Vec<u8>> {
        if self.byte_len > LIMIT as u64 || self.base64.len() > (LIMIT / 3 + 1) * 4 {
            return Err(KnowledgeError::budget_exceeded(
                "binary blob budget exceeded",
            ));
        }
        let bytes = STANDARD.decode(&self.base64)?;
        if bytes.len() as u64 != self.byte_len
            || sha256(&bytes) != self.hash
            || STANDARD.encode(&bytes) != self.base64
        {
            return Err(KnowledgeError::integrity(
                "noncanonical or corrupted binary blob",
            ));
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitEntry {
    pub commit_rev: i64,
    pub operation_id: String,
    pub kind: String,
    pub entity_id: i64,
    pub input_hash: String,
    pub receipt: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfOperationEntry {
    pub operation_id: String,
    pub kind: String,
    pub document_id: i64,
    pub capture_id: Option<i64>,
    pub input_hash: String,
    pub operation: String,
    pub receipt: String,
    pub outbox: String,
    pub index_job: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveV2 {
    pub schema_version: u32,
    pub text_entries: Vec<ExportEntry>,
    pub pdf_documents: Vec<PdfDocumentRecord>,
    pub pdf_captures: Vec<PdfCaptureRecord>,
    pub pdf_operations: Vec<PdfOperationEntry>,
    #[serde(default)]
    pub epub_documents: Vec<EpubDocumentRecord>,
    #[serde(default)]
    pub epub_captures: Vec<EpubCaptureRecord>,
    pub commits: Vec<CommitEntry>,
    pub blobs: Vec<BinaryBlob>,
}

pub type V2Archive = ArchiveV2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreAllReceipt {
    pub text_captures: usize,
    pub pdf_documents: usize,
    pub pdf_captures: usize,
    #[serde(default)]
    pub epub_documents: usize,
    #[serde(default)]
    pub epub_captures: usize,
}

impl ArchiveV2 {
    /// Compute the expected `ArchiveV2.schema_version` for this archive's record kinds.
    pub fn expected_schema_version(&self) -> u32 {
        if !self.epub_documents.is_empty() || !self.epub_captures.is_empty() {
            4
        } else if self
            .pdf_captures
            .iter()
            .any(|record| record.compound.is_some())
        {
            3
        } else {
            2
        }
    }

    /// Validate wire version, bounded size, and unique canonical blob decoding.
    pub fn validate_envelope(&self) -> Result<()> {
        self.decode_validated_blobs().map(|_| ())
    }

    #[doc(hidden)]
    pub fn decode_validated_blobs(&self) -> Result<BTreeMap<String, Vec<u8>>> {
        let expected_version = self.expected_schema_version();
        if self.schema_version != expected_version {
            return Err(KnowledgeError::unsupported_version(
                "unsupported or downgraded Core archive",
            ));
        }
        if self.commits.len() > 4096 || encode_json_bytes(self)?.len() > ARCHIVE_LIMIT {
            return Err(KnowledgeError::budget_exceeded("oversized Core archive"));
        }
        let mut blobs = BTreeMap::new();
        for blob in &self.blobs {
            let bytes = blob.decode()?;
            if blobs.insert(blob.hash.clone(), bytes).is_some() {
                return Err(KnowledgeError::integrity("duplicate archive blob"));
            }
        }
        Ok(blobs)
    }
}

/// Pure canonical JSON byte encoding helper shared with `learning-ports`.
#[doc(hidden)]
pub fn encode_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

/// Pure canonical JSON string encoding helper shared with `learning-ports`.
#[doc(hidden)]
pub fn encode_json_string<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

/// Validate bounded native capture inputs (operation id, vault, provenance, and note).
pub fn validate_native(op: &str, vault: &str, origin: &Provenance, user_note: &str) -> Result<()> {
    if user_note.len() > 1024 * 1024 {
        return Err(KnowledgeError::budget_exceeded("user note budget exceeded"));
    }
    if op.is_empty()
        || op.len() > 256
        || vault.is_empty()
        || vault.len() > 256
        || origin.installation_id < 1
        || origin.package_id.is_empty()
        || origin.package_id.len() > 256
        || origin.publisher_id.is_empty()
        || origin.publisher_id.len() > 256
        || !is_sha256_hex(&origin.package_digest)
    {
        return Err(KnowledgeError::invalid_input("invalid native commit input"));
    }
    Ok(())
}

/// Validate bounded native document import inputs (operation id, vault, and title).
pub fn native_import(op: &str, vault: &str, title: &str) -> Result<()> {
    if op.is_empty()
        || op.len() > 256
        || vault.is_empty()
        || vault.len() > 256
        || !is_bounded_label(title, 1024, true)
    {
        return Err(KnowledgeError::invalid_input("invalid PDF import input"));
    }
    Ok(())
}

/// Validate a mutable user note edit (bounded length and no disallowed control characters).
pub fn validate_note_input(note: &str) -> Result<()> {
    if note.len() > 1024 * 1024 {
        return Err(KnowledgeError::budget_exceeded("user note budget exceeded"));
    }
    if !is_bounded_text(note, 1024 * 1024, false, true, true) {
        return Err(KnowledgeError::invalid_input(
            "user note contains unsupported control characters",
        ));
    }
    Ok(())
}

/// Compute the canonical draft hash for either a single or compound PDF capture record.
pub fn capture_draft_hash(record: &PdfCaptureRecord) -> Result<String> {
    match &record.compound {
        Some(compound) => Ok(compound.draft_hash()?),
        None => Ok(record.draft.draft_hash()?),
    }
}

/// Serialize the canonical anchor JSON for either a single or compound PDF capture record.
pub fn capture_anchor(record: &PdfCaptureRecord) -> Result<String> {
    match &record.compound {
        Some(compound) => encode_json_string(compound),
        None => encode_json_string(&record.draft),
    }
}

/// Compatibility shim for external callers; checks wire envelope structure only.
/// Full cross-entity, commit-log, outbox, and index-job verification lives in
/// `learning_ports::archive::validate_archive`.
#[deprecated(
    note = "full archive verification moved to learning_ports::archive::validate_archive; this shim checks envelope structure only"
)]
pub fn validate_archive(archive: &ArchiveV2) -> Result<()> {
    archive.validate_envelope()
}
