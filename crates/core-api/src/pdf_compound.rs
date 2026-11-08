//! Explicit, ordered PDF line segments; this is not a reading-order inference.
//!
//! The supported profile names a qualified trusted adapter's presentation rule.
//! A plugin's profile declaration proves no such qualification or user approval.
//! Separators are presentation text, never invented characters in a source map.
use crate::{
    contract::sha256,
    pdf_contract::{
        MAX_PDF_REQUEST_BYTES, PdfContractError, PdfContractErrorCode, PdfDraft, PdfSelection,
        PdfSnapshot, Result,
    },
};
use serde::{Deserialize, Serialize};

pub const PDF_COMPOUND_PROTOCOL_VERSION: u16 = 2;
pub const PDF_SELECTION_PROFILE: &str = "qusor-plain-ltr-lines-v1";
pub const MAX_PDF_SEGMENTS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfCompoundDraft {
    pub protocol_version: u16,
    pub original_sha256: String,
    pub extraction_revision: String,
    pub selection_profile: String,
    pub separator: String,
    pub segments: Vec<PdfDraft>,
    pub exact: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfCompoundSelection {
    pub draft: PdfCompoundDraft,
    pub segments: Vec<PdfSelection>,
    pub exact: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfCompoundCaptureRequest {
    pub capture_protocol: u16,
    /// Correlation only; native identity, operation and approval are not wire input.
    pub request_id: String,
    pub draft: PdfCompoundDraft,
}
fn err(code: PdfContractErrorCode, message: &'static str) -> PdfContractError {
    PdfContractError { code, message }
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value).map_err(|_| {
        err(
            PdfContractErrorCode::Malformed,
            "cannot encode compound PDF JSON",
        )
    })?;
    if bytes.len() > MAX_PDF_REQUEST_BYTES {
        return Err(err(
            PdfContractErrorCode::BudgetExceeded,
            "compound PDF wire exceeds 4 KiB",
        ));
    }
    Ok(bytes)
}
fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.len() > MAX_PDF_REQUEST_BYTES {
        return Err(err(
            PdfContractErrorCode::BudgetExceeded,
            "compound PDF wire exceeds 4 KiB",
        ));
    }
    serde_json::from_slice(bytes).map_err(|_| {
        err(
            PdfContractErrorCode::Malformed,
            "invalid, unknown or duplicate compound PDF JSON fields",
        )
    })
}
impl PdfCompoundDraft {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Self = parse(bytes)?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        use PdfContractErrorCode::*;
        if self.protocol_version != PDF_COMPOUND_PROTOCOL_VERSION {
            return Err(err(UnsupportedVersion, "unsupported compound PDF protocol"));
        }
        if self.selection_profile != PDF_SELECTION_PROFILE || self.separator != "\n" {
            return Err(err(
                InvalidSelector,
                "unsupported compound PDF presentation profile",
            ));
        }
        if !(2..=MAX_PDF_SEGMENTS).contains(&self.segments.len()) {
            return Err(err(
                InvalidSelector,
                "compound PDF requires two to eight segments",
            ));
        }
        // Check the full encoded budget before concatenating any quotes.
        encode(self)?;
        let mut previous_page = None;
        for (index, segment) in self.segments.iter().enumerate() {
            segment.validate()?;
            if segment.original_sha256 != self.original_sha256
                || segment.extraction_revision != self.extraction_revision
            {
                return Err(err(
                    HashMismatch,
                    "compound PDF segments have different immutable sources",
                ));
            }
            if previous_page.is_some_and(|page| segment.page_index < page) {
                return Err(err(
                    InvalidSelector,
                    "compound PDF page order must be nondecreasing",
                ));
            }
            // Engine indices may run backwards between visual lines on rotated pages.
            // Compare every earlier range on this page rather than assuming monotonicity.
            if self.segments[..index].iter().any(|earlier| {
                earlier.page_index == segment.page_index
                    && earlier.start < segment.end
                    && segment.start < earlier.end
            }) {
                return Err(err(
                    InvalidSelector,
                    "compound PDF segments overlap or repeat",
                ));
            }
            previous_page = Some(segment.page_index);
        }
        let joined = self
            .segments
            .iter()
            .map(|segment| segment.exact.as_str())
            .collect::<Vec<_>>()
            .join(&self.separator);
        if joined != self.exact {
            return Err(err(
                QuoteMismatch,
                "compound PDF exact text differs from its ordered segments",
            ));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode(self)
    }
    pub fn draft_hash(&self) -> Result<String> {
        Ok(sha256(&self.canonical_bytes()?))
    }
    pub fn resolve(&self, snapshot: &PdfSnapshot) -> Result<PdfCompoundSelection> {
        self.validate()?;
        let segments = self
            .segments
            .iter()
            .map(|segment| snapshot.resolve(segment))
            .collect::<Result<Vec<_>>>()?;
        Ok(PdfCompoundSelection {
            draft: self.clone(),
            segments,
            exact: self.exact.clone(),
        })
    }
}
impl PdfCompoundCaptureRequest {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Self = parse(bytes)?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        if self.capture_protocol != PDF_COMPOUND_PROTOCOL_VERSION {
            return Err(err(
                PdfContractErrorCode::UnsupportedVersion,
                "unsupported compound PDF capture protocol",
            ));
        }
        if self.request_id.is_empty()
            || self.request_id.len() > 128
            || !self
                .request_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
        {
            return Err(err(
                PdfContractErrorCode::Malformed,
                "invalid compound PDF correlation id",
            ));
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
/// Resolve every source segment without fuzzy matching or inferred reordering.
pub fn resolve_pdf_compound(
    snapshot: &PdfSnapshot,
    draft: &PdfCompoundDraft,
) -> Result<PdfCompoundSelection> {
    draft.resolve(snapshot)
}
