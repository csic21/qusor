//! Canonical archive, commit-log, outbox, and index-job envelope assembly and
//! full-store verification for persistence ports and adapters.
use core_api::{
    contract::sha256,
    epub_contract::resolve_epub,
    error::{KnowledgeError, KnowledgeResult as Result},
    knowledge::{
        ArchiveV2, BinaryBlob, Blob, CaptureRecord, CommitEntry, EpubCaptureRecord,
        EpubDocumentRecord, ExportBundle, ExportEntry, MAX_ORIGINAL, PdfCaptureRecord,
        PdfDocumentRecord, PdfOperationEntry, encode_json_bytes, encode_json_string, native_import,
        validate_native,
    },
    pdf_contract::resolve_pdf,
};
use std::collections::{BTreeMap, BTreeSet};

fn integrity<T>(message: &str) -> Result<T> {
    Err(KnowledgeError::integrity(message))
}

/// Compute the expected `ArchiveV2.schema_version` for the archive's features.
pub fn archive_schema_version(archive: &ArchiveV2) -> u32 {
    archive.expected_schema_version()
}

/// Upgrade a legacy schema-1 text `ExportBundle` into an `ArchiveV2`.
pub fn archive_from_legacy(bundle: &ExportBundle) -> Result<ArchiveV2> {
    validate_bundle(bundle)?;
    Ok(ArchiveV2 {
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

pub fn input_hash(r: &CaptureRecord) -> Result<String> {
    Ok(sha256(&encode_json_bytes(&(
        &r.vault,
        &r.draft,
        &r.provenance,
        &r.user_note,
    ))?))
}

pub fn event(r: &CaptureRecord) -> Result<String> {
    encode_json_string(&(
        &r.receipt.operation_id,
        r.receipt.capture_id,
        r.receipt.document_version_id,
        &r.receipt.snapshot_hash,
    ))
}

pub fn validate_bundle(b: &ExportBundle) -> Result<()> {
    validate_text_bundle(b, true)
}

pub fn validate_text_bundle(b: &ExportBundle, legacy_revisions: bool) -> Result<()> {
    b.validate_envelope()?;
    let mut hashes = BTreeMap::new();
    for blob in &b.blobs {
        if sha256(blob.text.as_bytes()) != blob.hash
            || hashes.insert(&blob.hash, &blob.text).is_some()
        {
            return integrity("bad or duplicate blob");
        }
    }
    let mut ops = BTreeSet::new();
    let mut used = BTreeSet::new();
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
            return integrity("invalid export relationships or receipt");
        }
        used.insert(&r.receipt.snapshot_hash);
    }
    if used.len() != hashes.len() {
        return integrity("unreferenced export blob");
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
        receipt: encode_json_string(&entry.record.receipt)?,
    })
}

pub fn document_input(record: &PdfDocumentRecord) -> Result<String> {
    Ok(sha256(&encode_json_bytes(&(
        "pdf_import",
        &record.vault,
        &record.title,
        &record.snapshot,
    ))?))
}

pub fn capture_input(record: &PdfCaptureRecord, document: &PdfDocumentRecord) -> Result<String> {
    if let Some(compound) = &record.compound {
        return Ok(sha256(&encode_json_bytes(&(
            "pdf_capture_compound",
            record.receipt.document_id,
            &document.vault,
            compound,
            &record.provenance,
            &record.user_note,
        ))?));
    }
    Ok(sha256(&encode_json_bytes(&(
        "pdf_capture",
        record.receipt.document_id,
        &document.vault,
        &record.draft,
        &record.provenance,
        &record.user_note,
    ))?))
}

pub fn operation_for_document(record: &PdfDocumentRecord) -> Result<PdfOperationEntry> {
    let payload = encode_json_string(&(
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
        receipt: encode_json_string(&record.receipt)?,
        outbox: payload.clone(),
        index_job: payload,
    })
}

pub fn operation_for_capture(
    record: &PdfCaptureRecord,
    document: &PdfDocumentRecord,
) -> Result<PdfOperationEntry> {
    let payload = if let Some(compound) = &record.compound {
        encode_json_string(&(
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
        encode_json_string(&(
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
        receipt: encode_json_string(&record.receipt)?,
        outbox: payload.clone(),
        index_job: payload,
    })
}

pub fn validate_capture(document: &PdfDocumentRecord, record: &PdfCaptureRecord) -> Result<()> {
    match &record.compound {
        Some(compound) => {
            if compound.segments.first() != Some(&record.draft) {
                return integrity("compound PDF first-segment projection mismatch");
            }
            compound.resolve(&document.snapshot)?;
        }
        None => {
            resolve_pdf(&document.snapshot, &record.draft)?;
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

/// Compute canonical `input_hash` for an EPUB document import record.
pub fn epub_document_input(record: &EpubDocumentRecord) -> Result<String> {
    Ok(sha256(&encode_json_bytes(&(
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
    Ok(sha256(&encode_json_bytes(&(
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
        receipt: encode_json_string(&record.receipt)?,
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
        receipt: encode_json_string(&record.receipt)?,
    })
}

/// Validate a complete Core archive including blobs, cross-entity links,
/// outbox/index-job payloads, and monotonic commit history.
pub fn validate_archive(archive: &ArchiveV2) -> Result<()> {
    let blobs = archive.decode_validated_blobs()?;
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

    validate_pdf_records(
        archive,
        &blobs,
        &mut referenced,
        &mut expected_commits,
        &mut expected_operations,
    )?;
    validate_epub_records(archive, &mut expected_commits)?;

    let actual_operations: BTreeMap<_, _> = archive
        .pdf_operations
        .iter()
        .map(|o| (o.operation_id.clone(), o.clone()))
        .collect();
    if actual_operations.len() != archive.pdf_operations.len()
        || actual_operations != expected_operations
    {
        return integrity("PDF operation/receipt/audit/outbox/index mismatch");
    }
    expected_commits.sort_by_key(|c| c.commit_rev);
    if expected_commits != archive.commits {
        return integrity("global commit log mismatch");
    }
    let mut operations = BTreeSet::new();
    let mut prev_rev = 0i64;
    for commit in &archive.commits {
        if commit.commit_rev <= prev_rev || !operations.insert(&commit.operation_id) {
            return integrity("global commit revision or operation conflict");
        }
        prev_rev = commit.commit_rev;
    }
    if blobs.keys().cloned().collect::<BTreeSet<_>>() != referenced {
        return integrity("unreferenced or missing archive blob");
    }
    Ok(())
}

fn validate_pdf_records(
    archive: &ArchiveV2,
    blobs: &BTreeMap<String, Vec<u8>>,
    referenced: &mut BTreeSet<String>,
    expected_commits: &mut Vec<CommitEntry>,
    expected_operations: &mut BTreeMap<String, PdfOperationEntry>,
) -> Result<()> {
    let mut pdf_doc_ids = BTreeSet::new();
    for record in &archive.pdf_documents {
        let r = &record.receipt;
        native_import(&r.operation_id, &record.vault, &record.title)?;
        if r.document_id <= 0
            || !pdf_doc_ids.insert(r.document_id)
            || r.extraction_revision != record.snapshot.extraction_revision()?
            || r.original_sha256 != record.snapshot.original_sha256
        {
            return integrity("invalid PDF document identity");
        }
        let bytes = blobs
            .get(&r.original_sha256)
            .ok_or("PDF original missing")?;
        if bytes.len() > MAX_ORIGINAL {
            return integrity("PDF original budget exceeded");
        }
        record.snapshot.validate_original(bytes)?;
        referenced.insert(r.original_sha256.clone());
        let op = operation_for_document(record)?;
        expected_commits.push(commit_for_pdf(&op, r.commit_rev));
        if expected_operations
            .insert(op.operation_id.clone(), op)
            .is_some()
        {
            return integrity("duplicate PDF import operation");
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
            || r.draft_hash != core_api::knowledge::capture_draft_hash(record)?
            || r.commit_rev <= document.receipt.commit_rev
        {
            return integrity("invalid PDF capture identity");
        }
        validate_capture(document, record)?;
        let op = operation_for_capture(record, document)?;
        expected_commits.push(commit_for_pdf(&op, r.commit_rev));
        if expected_operations
            .insert(op.operation_id.clone(), op)
            .is_some()
        {
            return integrity("duplicate PDF capture operation");
        }
    }
    Ok(())
}

fn validate_epub_records(
    archive: &ArchiveV2,
    expected_commits: &mut Vec<CommitEntry>,
) -> Result<()> {
    let mut epub_doc_ids = BTreeSet::new();
    let mut epub_snapshots = BTreeMap::new();
    for record in &archive.epub_documents {
        let r = &record.receipt;
        native_import(&r.operation_id, &record.vault, &record.title)?;
        if r.document_id <= 0 || !epub_doc_ids.insert(r.document_id) {
            return integrity("invalid EPUB document identity");
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
            return integrity("invalid EPUB capture identity");
        }
        resolve_epub(snapshot, &record.draft)?;
        expected_commits.push(commit_for_epub_capture(record, document)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_api::{
        contract::{
            CaptureDraft, ExternalReturn, NORMALIZATION, PROTOCOL_VERSION, SourceDescriptor,
            TEXT_UNIT, TextSelector,
        },
        knowledge::{KnowledgeErrorCode, Provenance, Receipt},
    };

    #[test]
    fn validate_archive_rejects_tampered_outbox_and_index_job() {
        let quote = "Alpha beta";
        let draft = CaptureDraft {
            protocol_version: PROTOCOL_VERSION,
            source: SourceDescriptor {
                namespace: "notes".into(),
                reference: "ref-1".into(),
                title: "Title".into(),
                public_uri: None,
                external_return: ExternalReturn::None,
            },
            snapshot_text: quote.into(),
            selector: TextSelector {
                unit: TEXT_UNIT.into(),
                normalization: NORMALIZATION.into(),
                start: Some(0),
                end: Some(quote.chars().count() as u64),
                exact: quote.into(),
            },
        };
        let provenance = Provenance {
            package_id: "org.example.notes".into(),
            publisher_id: "org.example".into(),
            package_digest: "a".repeat(64),
            installation_id: 1,
        };
        let receipt = Receipt {
            operation_id: "op-1".into(),
            capture_id: 1,
            source_id: 1,
            document_version_id: 1,
            commit_rev: 1,
            snapshot_hash: draft.snapshot_hash(),
            draft_hash: draft.draft_hash().unwrap(),
        };
        let record = CaptureRecord {
            receipt,
            vault: "vault".into(),
            draft: draft.clone(),
            provenance,
            user_note: "note".into(),
        };
        let ev = event(&record).unwrap();
        let entry = ExportEntry {
            record,
            input_hash: input_hash(&CaptureRecord {
                receipt: Receipt {
                    operation_id: "op-1".into(),
                    capture_id: 1,
                    source_id: 1,
                    document_version_id: 1,
                    commit_rev: 1,
                    snapshot_hash: draft.snapshot_hash(),
                    draft_hash: draft.draft_hash().unwrap(),
                },
                vault: "vault".into(),
                draft: draft.clone(),
                provenance: Provenance {
                    package_id: "org.example.notes".into(),
                    publisher_id: "org.example".into(),
                    package_digest: "a".repeat(64),
                    installation_id: 1,
                },
                user_note: "note".into(),
            })
            .unwrap(),
            operation: ev.clone(),
            outbox: ev.clone(),
            index_job: ev,
        };
        let commit = text_commit(&entry).unwrap();
        let valid = ArchiveV2 {
            schema_version: 2,
            text_entries: vec![entry],
            pdf_documents: vec![],
            pdf_captures: vec![],
            pdf_operations: vec![],
            epub_documents: vec![],
            epub_captures: vec![],
            commits: vec![commit],
            blobs: vec![BinaryBlob::new(quote.as_bytes())],
        };
        assert!(validate_archive(&valid).is_ok());

        let mut bad_outbox = valid.clone();
        bad_outbox.text_entries[0].outbox = "tampered".into();
        assert_eq!(
            validate_archive(&bad_outbox).unwrap_err().code,
            KnowledgeErrorCode::Integrity
        );

        let mut bad_index = valid;
        bad_index.text_entries[0].index_job = "tampered".into();
        assert_eq!(
            validate_archive(&bad_index).unwrap_err().code,
            KnowledgeErrorCode::Integrity
        );
    }
}
