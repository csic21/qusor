//! Immutable application source facts, never a plugin authorization or Surface lease.
use crate::knowledge::KnowledgeApplication;
use core_api::{
    knowledge::{
        KnowledgeError, KnowledgeErrorCode, PdfReceipt, Provenance, Result, validate_native,
    },
    pdf_contract::{PdfDraft, resolve_pdf},
};
use learning_ports::{KnowledgeCommand, KnowledgeQuery, OperationLookup};

/// A source checked against Core before the selection's transient lease expires.
/// The trusted caller must check current capture authority again before `capture`.
/// No session, permit, renderer position, or editable source fields are retained.
#[derive(Debug)]
pub struct FrozenPdfSource {
    document_id: i64,
    vault: String,
    selection: PdfDraft,
    provenance: Provenance,
}

impl FrozenPdfSource {
    /// Call under the same trusted selection/authority gate as the freeze intent.
    /// Core checks content; this method does not authenticate a plugin identity.
    pub fn freeze<S: KnowledgeQuery>(
        app: &KnowledgeApplication<S>,
        document_id: i64,
        vault: &str,
        selection: PdfDraft,
        provenance: Provenance,
    ) -> Result<Self> {
        validate_native("annotation-freeze", vault, &provenance, "")?;
        let document = app.query_document(document_id)?;
        if document.vault != vault {
            return conflict("annotation source belongs to another vault");
        }
        resolve_pdf(&document.snapshot, &selection)?;
        Ok(Self {
            document_id,
            vault: vault.into(),
            selection,
            provenance,
        })
    }

    pub fn vault(&self) -> &str {
        &self.vault
    }
    pub fn document_id(&self) -> i64 {
        self.document_id
    }
    pub fn selection(&self) -> &PdfDraft {
        &self.selection
    }
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Requires fresh Host authorization for the original capture installation.
    /// The existing Core transaction rechecks exact source and operation identity.
    pub fn capture<S: KnowledgeQuery + KnowledgeCommand>(
        &self,
        app: &mut KnowledgeApplication<S>,
        operation_id: &str,
        note: &str,
    ) -> Result<PdfReceipt> {
        let document = app.query_document(self.document_id)?;
        if document.vault != self.vault {
            return conflict("annotation source vault changed");
        }
        app.capture_pdf(
            operation_id,
            self.document_id,
            &self.selection,
            &self.provenance,
            note,
        )
    }

    /// Trusted read-only reconciliation of a previously frozen save payload.
    /// No save permission is required to discover an already committed result.
    /// `None` means no operation exists in this consistent Core query; it is
    /// not permission to invent another operation or unlock a pending upload.
    pub fn receipt<S: KnowledgeQuery>(
        &self,
        app: &KnowledgeApplication<S>,
        operation_id: &str,
        note: &str,
    ) -> Result<Option<PdfReceipt>> {
        validate_native(operation_id, &self.vault, &self.provenance, note)?;
        match app.pdf_capture_operation(operation_id)? {
            OperationLookup::Match(found) => {
                let record = &found.record;
                if record.receipt.document_id != self.document_id
                    || record.draft != self.selection
                    || record.compound.is_some()
                    || record.provenance != self.provenance
                    || record.user_note != note
                    || found.document_vault != self.vault
                {
                    return conflict("annotation receipt belongs to a different payload");
                }
                Ok(Some(found.record.receipt))
            }
            OperationLookup::OtherOperationKind => {
                conflict("annotation operation belongs to another kind")
            }
            OperationLookup::Absent => Ok(None),
        }
    }
}
fn conflict<T>(message: &'static str) -> Result<T> {
    Err(KnowledgeError::new(KnowledgeErrorCode::Conflict, message))
}

/// Complete ordered PDF compound facts. Segments and presentation separators are
/// frozen together, never projected to a first segment or renderer coordinates.
#[derive(Debug)]
pub struct FrozenPdfCompoundSource {
    document_id: i64,
    vault: String,
    selection: core_api::pdf_compound::PdfCompoundDraft,
    provenance: Provenance,
}
impl FrozenPdfCompoundSource {
    pub fn freeze<S: KnowledgeQuery>(
        app: &KnowledgeApplication<S>,
        document_id: i64,
        vault: &str,
        selection: core_api::pdf_compound::PdfCompoundDraft,
        provenance: Provenance,
    ) -> Result<Self> {
        validate_native("annotation-freeze", vault, &provenance, "")?;
        let document = app.query_document(document_id)?;
        if document.vault != vault {
            return conflict("annotation source belongs to another vault");
        }
        core_api::pdf_compound::resolve_pdf_compound(&document.snapshot, &selection)?;
        Ok(Self {
            document_id,
            vault: vault.into(),
            selection,
            provenance,
        })
    }
    pub fn vault(&self) -> &str {
        &self.vault
    }
    pub fn document_id(&self) -> i64 {
        self.document_id
    }
    pub fn selection(&self) -> &core_api::pdf_compound::PdfCompoundDraft {
        &self.selection
    }
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
    /// Fresh original capture-installation authorization remains the caller's responsibility.
    pub fn capture<S: KnowledgeQuery + KnowledgeCommand>(
        &self,
        app: &mut KnowledgeApplication<S>,
        operation_id: &str,
        note: &str,
    ) -> Result<PdfReceipt> {
        if app.query_document(self.document_id)?.vault != self.vault {
            return conflict("annotation source vault changed");
        }
        app.capture_pdf_compound(
            operation_id,
            self.document_id,
            &self.selection,
            &self.provenance,
            note,
        )
    }
    pub fn receipt<S: KnowledgeQuery>(
        &self,
        app: &KnowledgeApplication<S>,
        operation_id: &str,
        note: &str,
    ) -> Result<Option<PdfReceipt>> {
        validate_native(operation_id, &self.vault, &self.provenance, note)?;
        match app.pdf_capture_operation(operation_id)? {
            OperationLookup::Match(found) => {
                let record = &found.record;
                if record.receipt.document_id != self.document_id
                    || record.compound.as_ref() != Some(&self.selection)
                    || self.selection.segments.first() != Some(&record.draft)
                    || record.provenance != self.provenance
                    || record.user_note != note
                    || found.document_vault != self.vault
                {
                    return conflict("annotation receipt belongs to a different payload");
                }
                Ok(Some(found.record.receipt))
            }
            OperationLookup::OtherOperationKind => {
                conflict("annotation operation belongs to another kind")
            }
            OperationLookup::Absent => Ok(None),
        }
    }
}

/// Kind-preserving session source; this is not a permit, lease or persistent UI model.
#[derive(Debug)]
pub enum FrozenAnnotationSource {
    Single(FrozenPdfSource),
    Compound(FrozenPdfCompoundSource),
}
impl FrozenAnnotationSource {
    pub fn single(&self) -> Option<&FrozenPdfSource> {
        match self {
            Self::Single(s) => Some(s),
            Self::Compound(_) => None,
        }
    }
    pub fn compound(&self) -> Option<&FrozenPdfCompoundSource> {
        match self {
            Self::Compound(s) => Some(s),
            Self::Single(_) => None,
        }
    }
    pub fn vault(&self) -> &str {
        match self {
            Self::Single(s) => s.vault(),
            Self::Compound(s) => s.vault(),
        }
    }
    pub fn document_id(&self) -> i64 {
        match self {
            Self::Single(s) => s.document_id(),
            Self::Compound(s) => s.document_id(),
        }
    }
    pub fn provenance(&self) -> &Provenance {
        match self {
            Self::Single(s) => s.provenance(),
            Self::Compound(s) => s.provenance(),
        }
    }
    pub fn exact(&self) -> &str {
        match self {
            Self::Single(s) => &s.selection().exact,
            Self::Compound(s) => &s.selection().exact,
        }
    }
    /// These top-level immutable source identities were resolved against Core;
    /// compound getters never infer a document identity from the first segment.
    pub fn original_sha256(&self) -> &str {
        match self {
            Self::Single(s) => &s.selection().original_sha256,
            Self::Compound(s) => &s.selection().original_sha256,
        }
    }
    pub fn extraction_revision(&self) -> &str {
        match self {
            Self::Single(s) => &s.selection().extraction_revision,
            Self::Compound(s) => &s.selection().extraction_revision,
        }
    }
    pub fn capture<S: KnowledgeQuery + KnowledgeCommand>(
        &self,
        app: &mut KnowledgeApplication<S>,
        operation: &str,
        note: &str,
    ) -> Result<PdfReceipt> {
        match self {
            Self::Single(s) => s.capture(app, operation, note),
            Self::Compound(s) => s.capture(app, operation, note),
        }
    }
    pub fn receipt<S: KnowledgeQuery>(
        &self,
        app: &KnowledgeApplication<S>,
        operation: &str,
        note: &str,
    ) -> Result<Option<PdfReceipt>> {
        match self {
            Self::Single(s) => s.receipt(app, operation, note),
            Self::Compound(s) => s.receipt(app, operation, note),
        }
    }
}
