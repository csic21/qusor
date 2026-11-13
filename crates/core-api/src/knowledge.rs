//! Persistent knowledge envelopes and pure consistency invariants.
//! No filesystem, database, runtime, or UI implementation is part of this module.
use crate::contract::{CaptureDraft, sha256};
use crate::epub_contract::{
    EpubChapterSnapshot, EpubDraft, EpubSnapshot, EpubTocEntry, resolve_epub,
};
pub use crate::error::{KnowledgeError, KnowledgeErrorCode, KnowledgeResult as Result};
use crate::pdf_compound::PdfCompoundDraft;
use crate::pdf_contract::{PdfDraft, PdfSnapshot};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const LIMIT: usize = 64 * 1024 * 1024;
pub const ARCHIVE_LIMIT: usize = LIMIT;
pub const MAX_ORIGINAL: usize = 16 * 1024 * 1024;
fn fail<T>(message: &str) -> Result<T> {
    Err(KnowledgeError::new(KnowledgeErrorCode::Integrity, message))
}
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
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "binary blob budget exceeded",
            ));
        }
        let bytes = STANDARD.decode(&self.base64)?;
        if bytes.len() as u64 != self.byte_len
            || sha256(&bytes) != self.hash
            || STANDARD.encode(&bytes) != self.base64
        {
            return fail("noncanonical or corrupted binary blob");
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
    pub fn from_legacy(bundle: &ExportBundle) -> Result<Self> {
        validate_bundle(bundle)?;
        Ok(Self {
            schema_version: 2,
            text_entries: bundle.entries.clone(),
            pdf_documents: vec![],
            pdf_captures: vec![],
            pdf_operations: vec![],
            epub_documents: vec![],
            epub_captures: vec![],
            commits: bundle
                .entries
                .iter()
                .map(text_commit)
                .collect::<Result<_>>()?,
            blobs: bundle
                .blobs
                .iter()
                .map(|b| BinaryBlob::new(b.text.as_bytes()))
                .collect(),
        })
    }
}

pub fn input_hash(r: &CaptureRecord) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(&(
        &r.vault,
        &r.draft,
        &r.provenance,
        &r.user_note,
    ))?))
}
pub fn event(r: &CaptureRecord) -> Result<String> {
    Ok(serde_json::to_string(&(
        &r.receipt.operation_id,
        r.receipt.capture_id,
        r.receipt.document_version_id,
        &r.receipt.snapshot_hash,
    ))?)
}
pub fn validate_bundle(b: &ExportBundle) -> Result<()> {
    validate_text_bundle(b, true)
}
pub fn validate_text_bundle(b: &ExportBundle, legacy_revisions: bool) -> Result<()> {
    if b.schema_version != 1 {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::UnsupportedVersion,
            "unsupported text archive schema",
        ));
    }
    if b.entries.len() > 4096 || serde_json::to_vec(b)?.len() > LIMIT {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::BudgetExceeded,
            "text export budget exceeded",
        ));
    }
    let mut hashes = std::collections::BTreeMap::new();
    for blob in &b.blobs {
        if sha256(blob.text.as_bytes()) != blob.hash
            || hashes.insert(&blob.hash, &blob.text).is_some()
        {
            return fail("bad or duplicate blob");
        }
    }
    let mut ops = std::collections::BTreeSet::new();
    let mut used = std::collections::BTreeSet::new();
    for (i, e) in b.entries.iter().enumerate() {
        let r = &e.record;
        let id = (i + 1) as i64;
        r.draft.validate()?;
        validate_native(
            &r.receipt.operation_id,
            &r.vault,
            &r.provenance,
            &r.user_note,
        )?;
        if r.receipt.capture_id != id
            || r.receipt.source_id != id
            || r.receipt.document_version_id != id
            || (legacy_revisions && r.receipt.commit_rev != id)
            || r.receipt.operation_id.is_empty()
            || !ops.insert(&r.receipt.operation_id)
            || r.receipt.snapshot_hash != r.draft.snapshot_hash()
            || r.receipt.draft_hash != r.draft.draft_hash()?
            || e.input_hash != input_hash(r)?
            || e.operation != event(r)?
            || e.outbox != event(r)?
            || e.index_job != event(r)?
            || hashes.get(&r.receipt.snapshot_hash) != Some(&&r.draft.snapshot_text)
        {
            return fail("invalid export relationships or receipt");
        }
        used.insert(&r.receipt.snapshot_hash);
    }
    if used.len() != hashes.len() {
        return fail("unreferenced export blob");
    }
    Ok(())
}
pub fn validate_native(op: &str, vault: &str, origin: &Provenance, user_note: &str) -> Result<()> {
    if user_note.len() > 1024 * 1024 {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::BudgetExceeded,
            "user note budget exceeded",
        ));
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
        || origin.package_digest.len() != 64
        || !origin
            .package_digest
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::InvalidInput,
            "invalid native commit input",
        ));
    }
    Ok(())
}
pub fn text_commit(entry: &ExportEntry) -> Result<CommitEntry> {
    Ok(CommitEntry {
        commit_rev: entry.record.receipt.commit_rev,
        operation_id: entry.record.receipt.operation_id.clone(),
        kind: "text".into(),
        entity_id: entry.record.receipt.capture_id,
        input_hash: entry.input_hash.clone(),
        receipt: serde_json::to_string(&entry.record.receipt)?,
    })
}
pub fn document_input(record: &PdfDocumentRecord) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(&(
        "pdf_import",
        &record.vault,
        &record.title,
        &record.snapshot,
    ))?))
}
pub fn capture_input(record: &PdfCaptureRecord, document: &PdfDocumentRecord) -> Result<String> {
    if let Some(compound) = &record.compound {
        return Ok(sha256(&serde_json::to_vec(&(
            "pdf_capture_compound",
            record.receipt.document_id,
            &document.vault,
            compound,
            &record.provenance,
            &record.user_note,
        ))?));
    }
    Ok(sha256(&serde_json::to_vec(&(
        "pdf_capture",
        record.receipt.document_id,
        &document.vault,
        &record.draft,
        &record.provenance,
        &record.user_note,
    ))?))
}
pub fn operation_for_document(record: &PdfDocumentRecord) -> Result<PdfOperationEntry> {
    let payload = serde_json::to_string(&(
        "pdf_import",
        &record.receipt.operation_id,
        record.receipt.document_id,
        record.receipt.commit_rev,
        &record.receipt.original_sha256,
        &record.receipt.extraction_revision,
    ))?;
    Ok(PdfOperationEntry {
        operation_id: record.receipt.operation_id.clone(),
        kind: "pdf_import".into(),
        document_id: record.receipt.document_id,
        capture_id: None,
        input_hash: document_input(record)?,
        operation: payload.clone(),
        receipt: serde_json::to_string(&record.receipt)?,
        outbox: payload.clone(),
        index_job: payload,
    })
}
pub fn operation_for_capture(
    record: &PdfCaptureRecord,
    document: &PdfDocumentRecord,
) -> Result<PdfOperationEntry> {
    let payload = if let Some(compound) = &record.compound {
        serde_json::to_string(&(
            "pdf_capture_compound",
            &record.receipt.operation_id,
            record.receipt.capture_id,
            record.receipt.document_id,
            record.receipt.commit_rev,
            &record.receipt.original_sha256,
            &record.receipt.extraction_revision,
            compound.draft_hash()?,
        ))?
    } else {
        serde_json::to_string(&(
            "pdf_capture",
            &record.receipt.operation_id,
            record.receipt.capture_id,
            record.receipt.document_id,
            record.receipt.commit_rev,
            &record.receipt.original_sha256,
            &record.receipt.extraction_revision,
        ))?
    };
    Ok(PdfOperationEntry {
        operation_id: record.receipt.operation_id.clone(),
        kind: "pdf_capture".into(),
        document_id: record.receipt.document_id,
        capture_id: Some(record.receipt.capture_id),
        input_hash: capture_input(record, document)?,
        operation: payload.clone(),
        receipt: serde_json::to_string(&record.receipt)?,
        outbox: payload.clone(),
        index_job: payload,
    })
}
pub fn capture_draft_hash(record: &PdfCaptureRecord) -> Result<String> {
    match &record.compound {
        Some(compound) => Ok(compound.draft_hash()?),
        None => Ok(record.draft.draft_hash()?),
    }
}
pub fn capture_anchor(record: &PdfCaptureRecord) -> Result<String> {
    Ok(match &record.compound {
        Some(compound) => serde_json::to_string(compound)?,
        None => serde_json::to_string(&record.draft)?,
    })
}
pub fn validate_capture(document: &PdfDocumentRecord, record: &PdfCaptureRecord) -> Result<()> {
    match &record.compound {
        Some(compound) => {
            if compound.segments.first() != Some(&record.draft) {
                return fail("compound PDF first-segment projection mismatch");
            }
            compound.resolve(&document.snapshot)?;
        }
        None => {
            crate::pdf_contract::resolve_pdf(&document.snapshot, &record.draft)?;
        }
    }
    Ok(())
}
pub fn commit_for_pdf(operation: &PdfOperationEntry, rev: i64) -> CommitEntry {
    CommitEntry {
        commit_rev: rev,
        operation_id: operation.operation_id.clone(),
        kind: operation.kind.clone(),
        entity_id: operation.capture_id.unwrap_or(operation.document_id),
        input_hash: operation.input_hash.clone(),
        receipt: operation.receipt.clone(),
    }
}
pub fn native_import(op: &str, vault: &str, title: &str) -> Result<()> {
    if op.is_empty()
        || op.len() > 256
        || vault.is_empty()
        || vault.len() > 256
        || title.is_empty()
        || title.len() > 1024
        || title.chars().any(char::is_control)
    {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::InvalidInput,
            "invalid PDF import input",
        ));
    }
    Ok(())
}

/// Validate a mutable user note edit (bounded length and no disallowed control characters).
pub fn validate_note_input(note: &str) -> Result<()> {
    if note.len() > 1024 * 1024 {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::BudgetExceeded,
            "user note budget exceeded",
        ));
    }
    if note
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::InvalidInput,
            "user note contains unsupported control characters",
        ));
    }
    Ok(())
}

/// Compute canonical `input_hash` for an EPUB document import record.
pub fn epub_document_input(record: &EpubDocumentRecord) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(&(
        "epub_import",
        &record.vault,
        &record.title,
        &record.author,
        &record.receipt.original_sha256,
        record.original_bytes,
        &record.receipt.extraction_revision,
        &record.chapters,
        &record.toc,
    ))?))
}

/// Compute canonical `input_hash` for an EPUB capture record.
pub fn epub_capture_input(
    record: &EpubCaptureRecord,
    document: &EpubDocumentRecord,
) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(&(
        "epub_capture",
        record.receipt.document_id,
        &document.vault,
        &record.draft,
        &record.provenance,
        &record.user_note,
    ))?))
}

/// Build the `CommitEntry` representing an EPUB document import.
pub fn commit_for_epub_document(record: &EpubDocumentRecord) -> Result<CommitEntry> {
    Ok(CommitEntry {
        commit_rev: record.receipt.commit_rev,
        operation_id: record.receipt.operation_id.clone(),
        kind: "epub_import".into(),
        entity_id: record.receipt.document_id,
        input_hash: epub_document_input(record)?,
        receipt: serde_json::to_string(&record.receipt)?,
    })
}

/// Build the `CommitEntry` representing an EPUB selection capture.
pub fn commit_for_epub_capture(
    record: &EpubCaptureRecord,
    document: &EpubDocumentRecord,
) -> Result<CommitEntry> {
    Ok(CommitEntry {
        commit_rev: record.receipt.commit_rev,
        operation_id: record.receipt.operation_id.clone(),
        kind: "epub_capture".into(),
        entity_id: record.receipt.capture_id,
        input_hash: epub_capture_input(record, document)?,
        receipt: serde_json::to_string(&record.receipt)?,
    })
}

/// Compute the expected `ArchiveV2.schema_version` for the archive's features.
pub fn archive_schema_version(archive: &ArchiveV2) -> u32 {
    if !archive.epub_documents.is_empty() || !archive.epub_captures.is_empty() {
        4
    } else if archive
        .pdf_captures
        .iter()
        .any(|record| record.compound.is_some())
    {
        3
    } else {
        2
    }
}

pub fn validate_archive(archive: &ArchiveV2) -> Result<()> {
    let expected_version = archive_schema_version(archive);
    if archive.schema_version != expected_version {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::UnsupportedVersion,
            "unsupported or downgraded Core archive",
        ));
    }
    if archive.commits.len() > 4096 || serde_json::to_vec(archive)?.len() > ARCHIVE_LIMIT {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::BudgetExceeded,
            "oversized Core archive",
        ));
    }
    let mut blobs = BTreeMap::new();
    for blob in &archive.blobs {
        let bytes = blob.decode()?;
        if blobs.insert(blob.hash.clone(), bytes).is_some() {
            return fail("duplicate archive blob");
        }
    }
    let text_blobs = archive
        .text_entries
        .iter()
        .map(|entry| {
            let hash = &entry.record.receipt.snapshot_hash;
            Ok(Blob {
                hash: hash.clone(),
                text: String::from_utf8(blobs.get(hash).ok_or("text blob missing")?.clone())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let text_blobs: BTreeMap<_, _> = text_blobs
        .into_iter()
        .map(|b| (b.hash.clone(), b))
        .collect();
    validate_text_bundle(
        &ExportBundle {
            schema_version: 1,
            entries: archive.text_entries.clone(),
            blobs: text_blobs.into_values().collect(),
        },
        false,
    )?;
    let mut expected_commits = archive
        .text_entries
        .iter()
        .map(text_commit)
        .collect::<Result<Vec<_>>>()?;
    let mut expected_operations = BTreeMap::new();
    let mut referenced: BTreeSet<String> = archive
        .text_entries
        .iter()
        .map(|e| e.record.receipt.snapshot_hash.clone())
        .collect();
    let mut pdf_doc_ids = BTreeSet::new();
    for record in &archive.pdf_documents {
        let r = &record.receipt;
        native_import(&r.operation_id, &record.vault, &record.title)?;
        if r.document_id <= 0
            || !pdf_doc_ids.insert(r.document_id)
            || r.extraction_revision != record.snapshot.extraction_revision()?
            || r.original_sha256 != record.snapshot.original_sha256
        {
            return fail("invalid PDF document identity");
        };
        let bytes = blobs
            .get(&r.original_sha256)
            .ok_or("PDF original missing")?;
        if bytes.len() > MAX_ORIGINAL {
            return fail("PDF original budget exceeded");
        };
        record.snapshot.validate_original(bytes)?;
        referenced.insert(r.original_sha256.clone());
        let op = operation_for_document(record)?;
        expected_commits.push(commit_for_pdf(&op, r.commit_rev));
        if expected_operations
            .insert(op.operation_id.clone(), op)
            .is_some()
        {
            return fail("duplicate PDF import operation");
        }
    }
    let mut pdf_cap_ids = BTreeSet::new();
    for record in &archive.pdf_captures {
        let r = &record.receipt;
        let document = archive
            .pdf_documents
            .iter()
            .find(|d| d.receipt.document_id == r.document_id)
            .ok_or("PDF document relationship missing")?;
        validate_native(
            &r.operation_id,
            &document.vault,
            &record.provenance,
            &record.user_note,
        )?;
        if r.capture_id <= 0
            || !pdf_cap_ids.insert(r.capture_id)
            || r.original_sha256 != document.receipt.original_sha256
            || r.extraction_revision != document.receipt.extraction_revision
            || r.draft_hash != capture_draft_hash(record)?
            || r.commit_rev <= document.receipt.commit_rev
        {
            return fail("invalid PDF capture identity");
        };
        validate_capture(document, record)?;
        let op = operation_for_capture(record, document)?;
        expected_commits.push(commit_for_pdf(&op, r.commit_rev));
        if expected_operations
            .insert(op.operation_id.clone(), op)
            .is_some()
        {
            return fail("duplicate PDF capture operation");
        }
    }
    let mut epub_doc_ids = BTreeSet::new();
    let mut epub_snapshots = BTreeMap::new();
    for record in &archive.epub_documents {
        let r = &record.receipt;
        native_import(&r.operation_id, &record.vault, &record.title)?;
        if r.document_id <= 0 || !epub_doc_ids.insert(r.document_id) {
            return fail("invalid EPUB document identity");
        }
        let snapshot = record.to_snapshot();
        snapshot.validate()?;
        epub_snapshots.insert(r.document_id, (record, snapshot));
        expected_commits.push(commit_for_epub_document(record)?);
    }
    let mut epub_cap_ids = BTreeSet::new();
    for record in &archive.epub_captures {
        let r = &record.receipt;
        let (document, snapshot) = epub_snapshots
            .get(&r.document_id)
            .ok_or("EPUB document relationship missing")?;
        validate_native(
            &r.operation_id,
            &document.vault,
            &record.provenance,
            &record.user_note,
        )?;
        if r.capture_id <= 0
            || !epub_cap_ids.insert(r.capture_id)
            || r.original_sha256 != document.receipt.original_sha256
            || r.extraction_revision != document.receipt.extraction_revision
            || r.draft_hash != record.draft.draft_hash()?
            || r.commit_rev <= document.receipt.commit_rev
        {
            return fail("invalid EPUB capture identity");
        }
        resolve_epub(snapshot, &record.draft)?;
        expected_commits.push(commit_for_epub_capture(record, document)?);
    }
    let actual_operations: BTreeMap<_, _> = archive
        .pdf_operations
        .iter()
        .map(|o| (o.operation_id.clone(), o.clone()))
        .collect();
    if actual_operations.len() != archive.pdf_operations.len()
        || actual_operations != expected_operations
    {
        return fail("PDF operation/receipt/audit/outbox/index mismatch");
    }
    expected_commits.sort_by_key(|c| c.commit_rev);
    if expected_commits != archive.commits {
        return fail("global commit log mismatch");
    };
    let mut operations = BTreeSet::new();
    let mut prev_rev = 0i64;
    for commit in &archive.commits {
        if commit.commit_rev <= prev_rev || !operations.insert(&commit.operation_id) {
            return fail("global commit revision or operation conflict");
        }
        prev_rev = commit.commit_rev;
    }
    if blobs.keys().cloned().collect::<BTreeSet<_>>() != referenced {
        return fail("unreferenced or missing archive blob");
    };
    Ok(())
}
