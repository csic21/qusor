//! Immutable PDF character-stream contract. No parser, filesystem, SQL, runtime or UI.
//!
//! `engine_text` is the provider's original character stream, not canonical reading
//! order. Every rectangle is unrotated PDF user space (x right, y up), independent
//! of viewport, pixels, zoom and DPI. Unknown Unicode mappings are preserved as
//! U+FFFD evidence but cannot establish an exact text anchor.
use crate::contract::sha256;
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

pub const PDF_PROTOCOL_VERSION: u16 = 1;
pub const MAX_PDF_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PDF_PAGES: usize = 16;
pub const MAX_PDF_CHARACTERS: usize = 100_000;
pub const MAX_PDF_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PDF_REQUEST_BYTES: usize = 4096;
pub const PDF_TEXT_UNIT: &str = "unicode_scalar";
pub const PDF_NORMALIZATION: &str = "pdfium-char-stream-raw-v1";
pub type Result<T> = std::result::Result<T, PdfContractError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfContractErrorCode {
    Malformed,
    BudgetExceeded,
    UnsupportedVersion,
    InvalidIdentity,
    InvalidGeometry,
    InvalidMap,
    UnknownMapping,
    HashMismatch,
    OriginalMismatch,
    InvalidSelector,
    InvalidBoundary,
    QuoteMismatch,
    NoText,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfContractError {
    pub code: PdfContractErrorCode,
    pub message: &'static str,
}
impl std::fmt::Display for PdfContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for PdfContractError {}
fn err(code: PdfContractErrorCode, message: &'static str) -> PdfContractError {
    PdfContractError { code, message }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionIdentity {
    pub provider: String,
    pub algorithm: String,
    pub wrapper: String,
    pub abi: String,
    pub library_sha256: String,
    pub mapping_check: String,
    pub normalization: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfRect {
    #[serde(serialize_with = "coordinate")]
    pub left: f32,
    #[serde(serialize_with = "coordinate")]
    pub bottom: f32,
    #[serde(serialize_with = "coordinate")]
    pub right: f32,
    #[serde(serialize_with = "coordinate")]
    pub top: f32,
}
fn coordinate<S: serde::Serializer>(
    value: &f32,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    // Signed zero has one geometric meaning and one canonical representation.
    serializer.serialize_f32(if *value == 0.0 { 0.0 } else { *value })
}
impl PdfRect {
    fn validate(&self, positive_area: bool) -> Result<()> {
        let finite = [self.left, self.bottom, self.right, self.top]
            .iter()
            .all(|v| v.is_finite());
        if !finite
            || self.left > self.right
            || self.bottom > self.top
            || (positive_area && (self.left == self.right || self.bottom == self.top))
        {
            return Err(err(
                PdfContractErrorCode::InvalidGeometry,
                "invalid PDF user-space rectangle",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfCharacter {
    pub pdf_char_index: u64,
    pub unicode_value: u32,
    /// Only `scalar` and `unknown` are supported by this profile.
    pub mapping: String,
    pub generated: bool,
    pub scalar_start: u64,
    pub scalar_end: u64,
    pub utf8_start: u64,
    pub utf8_end: u64,
    pub utf16_start: u64,
    pub utf16_end: u64,
    pub pdf_bounds: Option<PdfRect>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfPage {
    pub page_index: u32,
    pub engine_text: String,
    pub text_sha256: String,
    pub rotation_degrees: u16,
    pub media_box: PdfRect,
    pub crop_box: PdfRect,
    pub characters: Vec<PdfCharacter>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfSnapshot {
    pub protocol_version: u16,
    pub original_sha256: String,
    pub original_bytes: u64,
    pub extraction: ExtractionIdentity,
    /// Provenance evidence; excluded from the neutral semantic revision digest.
    pub engine_report_sha256: String,
    pub pages: Vec<PdfPage>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfDraft {
    pub protocol_version: u16,
    pub original_sha256: String,
    pub extraction_revision: String,
    pub page_index: u32,
    pub page_text_sha256: String,
    pub unit: String,
    pub start: u64,
    pub end: u64,
    pub exact: String,
}
/// An anchor is a fully constrained immutable draft, never an inferred remapping.
pub type PdfAnchor = PdfDraft;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfSelection {
    pub anchor: PdfAnchor,
    pub page_index: u32,
    pub exact: String,
    pub start: u64,
    pub end: u64,
    pub byte_start: u64,
    pub byte_end: u64,
    pub utf16_start: u64,
    pub utf16_end: u64,
    pub characters: Vec<PdfCharacter>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfCaptureRequest {
    pub capture_protocol: u16,
    /// Correlation only: not identity, authority, approval or a Core operation id.
    pub request_id: String,
    pub draft: PdfDraft,
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn version(value: u16) -> Result<()> {
    if value != PDF_PROTOCOL_VERSION {
        return Err(err(
            PdfContractErrorCode::UnsupportedVersion,
            "unsupported PDF protocol",
        ));
    }
    Ok(())
}
fn encode<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| err(PdfContractErrorCode::Malformed, "cannot encode PDF JSON"))?;
    if bytes.len() > limit {
        return Err(err(
            PdfContractErrorCode::BudgetExceeded,
            "PDF JSON exceeds budget",
        ));
    }
    Ok(bytes)
}
fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T> {
    if bytes.len() > limit {
        return Err(err(
            PdfContractErrorCode::BudgetExceeded,
            "PDF JSON exceeds budget",
        ));
    }
    serde_json::from_slice(bytes).map_err(|_| {
        err(
            PdfContractErrorCode::Malformed,
            "invalid, unknown or duplicate PDF JSON fields",
        )
    })
}
impl ExtractionIdentity {
    fn validate(&self) -> Result<()> {
        if self.provider != "pdfium"
            || self.algorithm != "qusor-pdfium-char-map-v1"
            || self.wrapper != "pdfium-render-0.9.5"
            || self.abi != "pdfium_8125"
            || self.mapping_check != "unicode_scalar_validity_only"
            || self.normalization != PDF_NORMALIZATION
            || !digest(&self.library_sha256)
        {
            return Err(err(
                PdfContractErrorCode::InvalidIdentity,
                "unsupported or invalid extraction identity",
            ));
        }
        Ok(())
    }
}
impl PdfSnapshot {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Self = parse(bytes, MAX_PDF_SNAPSHOT_BYTES)?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        use PdfContractErrorCode::*;
        version(self.protocol_version)?;
        if self.original_bytes == 0
            || self.original_bytes > MAX_PDF_BYTES as u64
            || self.pages.is_empty()
            || self.pages.len() > MAX_PDF_PAGES
        {
            return Err(err(BudgetExceeded, "PDF original/page budget invalid"));
        }
        if !digest(&self.original_sha256) || !digest(&self.engine_report_sha256) {
            return Err(err(HashMismatch, "invalid PDF digest"));
        }
        self.extraction.validate()?;
        let mut total = 0usize;
        for (page_index, page) in self.pages.iter().enumerate() {
            total = total
                .checked_add(page.characters.len())
                .ok_or_else(|| err(BudgetExceeded, "character count overflow"))?;
            if total > MAX_PDF_CHARACTERS || page.engine_text.len() > MAX_PDF_CHARACTERS * 4 {
                return Err(err(BudgetExceeded, "PDF character budget exceeded"));
            }
            if page.page_index as usize != page_index
                || ![0, 90, 180, 270].contains(&page.rotation_degrees)
            {
                return Err(err(InvalidMap, "invalid page index or rotation"));
            }
            page.media_box.validate(true)?;
            page.crop_box.validate(true)?;
            if page.text_sha256 != sha256(page.engine_text.as_bytes()) {
                return Err(err(HashMismatch, "page text hash mismatch"));
            }
            if page.engine_text.chars().count() != page.characters.len() {
                return Err(err(InvalidMap, "character map does not cover engine text"));
            }
            let mut utf16 = 0u64;
            for (index, ((byte, scalar), mapped)) in page
                .engine_text
                .char_indices()
                .zip(&page.characters)
                .enumerate()
            {
                let valid =
                    mapped.unicode_value != 0 && char::from_u32(mapped.unicode_value).is_some();
                let expected = if valid {
                    char::from_u32(mapped.unicode_value).unwrap()
                } else {
                    '\u{fffd}'
                };
                if scalar != expected
                    || mapped.mapping != if valid { "scalar" } else { "unknown" }
                    || mapped.pdf_char_index != index as u64
                    || mapped.scalar_start != index as u64
                    || mapped.scalar_end != index as u64 + 1
                    || mapped.utf8_start != byte as u64
                    || mapped.utf8_end != (byte + scalar.len_utf8()) as u64
                    || mapped.utf16_start != utf16
                    || mapped.utf16_end != utf16 + scalar.len_utf16() as u64
                {
                    return Err(err(
                        InvalidMap,
                        "inconsistent Unicode/character/offset mapping",
                    ));
                }
                // Missing geometry is explicit for generated separators and whitespace.
                if mapped.generated || scalar.is_whitespace() {
                    if mapped.pdf_bounds.is_some() {
                        return Err(err(
                            InvalidGeometry,
                            "generated/whitespace character has invented bounds",
                        ));
                    }
                } else if let Some(bounds) = &mapped.pdf_bounds {
                    bounds.validate(false)?;
                } else {
                    return Err(err(
                        InvalidGeometry,
                        "non-generated glyph is missing PDF bounds",
                    ));
                }
                utf16 += scalar.len_utf16() as u64;
            }
        }
        encode(self, MAX_PDF_SNAPSHOT_BYTES)?;
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode(self, MAX_PDF_SNAPSHOT_BYTES)
    }
    /// Immutable semantic revision: render-report provenance is stored separately.
    pub fn snapshot_hash(&self) -> Result<String> {
        self.validate()?;
        #[derive(Serialize)]
        struct Semantic<'a> {
            protocol_version: u16,
            original_sha256: &'a str,
            original_bytes: u64,
            extraction: &'a ExtractionIdentity,
            pages: &'a [PdfPage],
        }
        Ok(sha256(&encode(
            &Semantic {
                protocol_version: self.protocol_version,
                original_sha256: &self.original_sha256,
                original_bytes: self.original_bytes,
                extraction: &self.extraction,
                pages: &self.pages,
            },
            MAX_PDF_SNAPSHOT_BYTES,
        )?))
    }
    pub fn extraction_revision(&self) -> Result<String> {
        self.snapshot_hash()
    }
    pub fn validate_original(&self, original: &[u8]) -> Result<()> {
        self.validate()?;
        if original.len() > MAX_PDF_BYTES {
            return Err(err(
                PdfContractErrorCode::BudgetExceeded,
                "original PDF exceeds budget",
            ));
        }
        if original.len() as u64 != self.original_bytes || sha256(original) != self.original_sha256
        {
            return Err(err(
                PdfContractErrorCode::OriginalMismatch,
                "original PDF bytes do not match immutable snapshot",
            ));
        }
        Ok(())
    }
    pub fn resolve(&self, draft: &PdfDraft) -> Result<PdfSelection> {
        resolve_pdf(self, draft)
    }
}
impl PdfDraft {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Self = parse(bytes, MAX_PDF_REQUEST_BYTES)?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        use PdfContractErrorCode::*;
        version(self.protocol_version)?;
        if !digest(&self.original_sha256)
            || !digest(&self.extraction_revision)
            || !digest(&self.page_text_sha256)
        {
            return Err(err(HashMismatch, "invalid anchor digest"));
        }
        if self.page_index as usize >= MAX_PDF_PAGES
            || self.unit != PDF_TEXT_UNIT
            || self.start >= self.end
            || self.end > MAX_PDF_CHARACTERS as u64
            || self.exact.is_empty()
        {
            return Err(err(InvalidSelector, "invalid PDF scalar selector"));
        }
        encode(self, MAX_PDF_REQUEST_BYTES)?;
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode(self, MAX_PDF_REQUEST_BYTES)
    }
    pub fn draft_hash(&self) -> Result<String> {
        Ok(sha256(&self.canonical_bytes()?))
    }
}
impl PdfCaptureRequest {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Self = parse(bytes, MAX_PDF_REQUEST_BYTES)?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        version(self.capture_protocol)?;
        if self.request_id.is_empty()
            || self.request_id.len() > 128
            || !self
                .request_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        {
            return Err(err(
                PdfContractErrorCode::Malformed,
                "invalid PDF request correlation id",
            ));
        }
        self.draft.validate()?;
        encode(self, MAX_PDF_REQUEST_BYTES)?;
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode(self, MAX_PDF_REQUEST_BYTES)
    }
}
/// Resolve only the exact immutable source map; never search, reorder or substitute.
pub fn resolve_pdf(snapshot: &PdfSnapshot, draft: &PdfDraft) -> Result<PdfSelection> {
    use PdfContractErrorCode::*;
    snapshot.validate()?;
    draft.validate()?;
    if draft.original_sha256 != snapshot.original_sha256
        || draft.extraction_revision != snapshot.extraction_revision()?
    {
        return Err(err(HashMismatch, "original/extraction revision mismatch"));
    }
    let page = snapshot
        .pages
        .get(draft.page_index as usize)
        .ok_or_else(|| err(InvalidSelector, "page not in snapshot"))?;
    if draft.page_text_sha256 != page.text_sha256 {
        return Err(err(HashMismatch, "anchor page hash mismatch"));
    }
    if page.characters.is_empty() {
        return Err(err(NoText, "page has no engine text"));
    }
    let (start, end) = (draft.start as usize, draft.end as usize);
    let characters = page
        .characters
        .get(start..end)
        .ok_or_else(|| err(InvalidSelector, "range outside page"))?;
    if characters.iter().any(|c| c.mapping != "scalar") {
        return Err(err(
            UnknownMapping,
            "selection contains unknown Unicode mapping",
        ));
    }
    let first = characters
        .first()
        .ok_or_else(|| err(InvalidSelector, "empty selection"))?;
    let last = characters.last().unwrap();
    let (byte_start, byte_end) = (first.utf8_start as usize, last.utf8_end as usize);
    let boundary = |byte| {
        byte == page.engine_text.len()
            || page
                .engine_text
                .grapheme_indices(true)
                .any(|(i, _)| i == byte)
    };
    if !boundary(byte_start) || !boundary(byte_end) {
        return Err(err(
            InvalidBoundary,
            "PDF selection splits an extended grapheme",
        ));
    }
    if page.engine_text[byte_start..byte_end] != draft.exact {
        return Err(err(QuoteMismatch, "PDF range and exact quote disagree"));
    }
    Ok(PdfSelection {
        anchor: draft.clone(),
        page_index: draft.page_index,
        exact: draft.exact.clone(),
        start: draft.start,
        end: draft.end,
        byte_start: first.utf8_start,
        byte_end: last.utf8_end,
        utf16_start: first.utf16_start,
        utf16_end: last.utf16_end,
        characters: characters.to_vec(),
    })
}
