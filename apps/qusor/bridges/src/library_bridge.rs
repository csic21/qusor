//! Trusted adapter from `library-metadata-v1` to the Core Library read model.
//!
//! The Host binds the vault and authorizes `library.read`; this adapter maps
//! bounded metadata requests onto `learning-application` and never exposes PDF
//! bytes, paths, notes or database handles.
use core_api::knowledge::{KnowledgeError, KnowledgeErrorCode};
use extension_api::{
    ApiError, ErrorCode,
    library::{
        LibraryDocument, LibraryOperation, LibraryRequest, LibraryResponse, LibraryResult,
        MAX_LIBRARY_TITLE_BYTES,
    },
};
use learning_application::{KnowledgeApplication, pdf_library::PdfDocumentSummary};
use learning_ports::KnowledgeStore;

pub struct LibraryReadBridge<S: KnowledgeStore> {
    app: KnowledgeApplication<S>,
    vault: String,
}

impl<S: KnowledgeStore> LibraryReadBridge<S> {
    pub fn new(app: KnowledgeApplication<S>, vault: impl Into<String>) -> Self {
        Self {
            app,
            vault: vault.into(),
        }
    }

    pub fn vault(&self) -> &str {
        &self.vault
    }

    pub fn respond(&self, request: &LibraryRequest) -> LibraryResponse {
        let request_id = request.request_id.clone();
        let result = match &request.operation {
            LibraryOperation::Page { cursor, limit } => self.page(cursor.as_deref(), *limit),
        };
        let response = LibraryResponse { request_id, result };
        if response.to_json().is_ok() {
            response
        } else {
            LibraryResponse {
                request_id: response.request_id,
                result: LibraryResult::Error {
                    error: ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "library response exceeds the wire budget",
                    ),
                },
            }
        }
    }

    fn page(&self, cursor: Option<&str>, limit: u16) -> LibraryResult {
        match self
            .app
            .pdf_documents_page(&self.vault, cursor, usize::from(limit))
        {
            Ok(page) => LibraryResult::Page {
                documents: page.documents.iter().map(document).collect(),
                next_cursor: page.next_cursor,
            },
            Err(error) => LibraryResult::Error {
                error: map_error(&error),
            },
        }
    }
}

fn document(value: &PdfDocumentSummary) -> LibraryDocument {
    LibraryDocument {
        document_id: value.document_id,
        commit_rev: value.commit_rev,
        title: title(&value.title),
        original_sha256: value.original_sha256.clone(),
        original_bytes: value.original_bytes,
        extraction_revision: value.extraction_revision.clone(),
        page_count: value.page_count as u32,
    }
}

fn title(value: &str) -> String {
    let mut out = String::new();
    for character in value.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if out.len() + character.len_utf8() > MAX_LIBRARY_TITLE_BYTES {
            break;
        }
        out.push(character);
    }
    if out.is_empty() {
        "?".into()
    } else {
        out
    }
}

fn map_error(error: &KnowledgeError) -> ApiError {
    let code = match error.code {
        KnowledgeErrorCode::InvalidInput => ErrorCode::InvalidRequest,
        KnowledgeErrorCode::BudgetExceeded => ErrorCode::BudgetExceeded,
        KnowledgeErrorCode::NotFound => ErrorCode::NotFound,
        KnowledgeErrorCode::UnsupportedVersion => ErrorCode::UnsupportedVersion,
        KnowledgeErrorCode::Unavailable => ErrorCode::CapabilityUnavailable,
        _ => ErrorCode::InvalidState,
    };
    ApiError::new(code, "library read failed")
}
