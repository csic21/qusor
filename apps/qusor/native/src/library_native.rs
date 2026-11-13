//! Trusted Library Host mounted by the application composition runtime.
use crate::{epub_reader, package_catalog, profile, runtime, vault::VaultStore};
use core_api::{
    contract::sha256,
    epub_contract::{
        EpubChapterSnapshot, EpubSnapshot, EpubTocEntry, MAX_EPUB_AUTHOR_BYTES, MAX_EPUB_BYTES,
        MAX_EPUB_CHAPTER_BYTES, MAX_EPUB_CHAPTERS, MAX_EPUB_HREF_BYTES, MAX_EPUB_TITLE_BYTES,
        MAX_EPUB_TOC_DEPTH, MAX_EPUB_TOC_ENTRIES,
    },
    knowledge::Provenance,
};
use extension_api::{ApiError, Capability, HOST_API_V0_7, Resource, library::*};
use extension_host::{Grant, Host, Session};
use learning_application::{
    KnowledgeApplication,
    epub_library::{EpubSelectionRange, MAX_EPUB_TEXT_CHUNK},
};
use nsh_core::prelude::*;
use qusor_app_bridges::library_bridge::LibraryReadBridge;
use serde_json::{Value, json};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

pub(crate) struct LibraryRuntime {
    host: Host,
    session: Session,
    store: VaultStore,
    bridge: LibraryReadBridge<VaultStore>,
    provenance: Provenance,
}

impl LibraryRuntime {
    pub(crate) fn mount(store: VaultStore) -> Result<Self, String> {
        profile::require_active(profile::LIBRARY_HOST_ENTRY)?;
        let verified = package_catalog::verify("org.qusor.library")?;
        let mut host = Host::new(
            "qusor-vault",
            [Capability::LibraryRead, Capability::CapturePropose],
        )
        .map_err(text)?;
        let installation = host
            .register(verified.manifest().clone(), verified.digest())
            .map_err(text)?;
        host.replace_grants(
            installation,
            vec![
                Grant {
                    capability: Capability::LibraryRead,
                    resource: Resource::Vault {
                        vault_id: "qusor-vault".into(),
                    },
                },
                Grant {
                    capability: Capability::CapturePropose,
                    resource: Resource::Vault {
                        vault_id: "qusor-vault".into(),
                    },
                },
            ],
        )
        .map_err(text)?;
        host.enable(installation).map_err(text)?;
        let session = host
            .open_session_for_api(installation, HOST_API_V0_7)
            .map_err(text)?;
        let installation_id = i64::try_from(installation.sequence()).map_err(text)?;
        let provenance = Provenance {
            package_id: verified.manifest().package_id.clone(),
            publisher_id: verified.manifest().publisher_id.clone(),
            package_digest: verified.digest().to_string(),
            installation_id,
        };
        Ok(Self {
            host,
            session,
            store: store.clone(),
            bridge: LibraryReadBridge::new(KnowledgeApplication::new(store), "qusor-vault"),
            provenance,
        })
    }

    #[cfg(unix)]
    pub(crate) fn import_pdf(
        &self,
        sys: &System,
        title: String,
        bytes: Vec<u8>,
    ) -> Result<Value, String> {
        let imported = crate::pdf_reader::import(sys, self.store.clone(), title, bytes)?;
        Ok(crate::pdf_reader::import_value(imported))
    }

    #[cfg(unix)]
    pub(crate) fn render_pdf_page(
        &self,
        sys: &System,
        document_id: i64,
        page_index: u32,
    ) -> Result<crate::pdf_page::PdfPageData, String> {
        crate::pdf_reader::render_page(sys, self.store.clone(), document_id, page_index)
    }

    fn page(&mut self, cursor: Option<String>, limit: u16) -> Result<Value, String> {
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_7,
            request_id: "workbench-library-page".into(),
            operation: LibraryOperation::Page { cursor, limit },
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_library(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |context| Ok(bridge.respond(&request, context)))
            .map_err(text)?;
        match response.result {
            LibraryResult::Page {
                documents,
                next_cursor,
            } => Ok(json!({
                "documents": documents,
                "next_cursor": next_cursor,
            })),
            LibraryResult::PageText { .. } => Err("unexpected Library text result".into()),
            LibraryResult::Capture { .. } => Err("unexpected Library capture result".into()),
            LibraryResult::SavedSelections { .. } => {
                Err("unexpected Library saved-selection result".into())
            }
            LibraryResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn page_text(
        &mut self,
        document_id: i64,
        page_index: u32,
        offset: u32,
        max_bytes: u16,
    ) -> Result<Value, String> {
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_7,
            request_id: "workbench-library-page-text".into(),
            operation: LibraryOperation::PageText {
                document_id,
                page_index,
                offset,
                max_bytes,
            },
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_library(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |context| Ok(bridge.respond(&request, context)))
            .map_err(text)?;
        match response.result {
            LibraryResult::Page { .. } => Err("unexpected Library page result".into()),
            LibraryResult::Capture { .. } => Err("unexpected Library capture result".into()),
            LibraryResult::SavedSelections { .. } => {
                Err("unexpected Library saved-selection result".into())
            }
            LibraryResult::PageText {
                document_id,
                page_index,
                offset,
                total_bytes,
                text,
                next_offset,
            } => Ok(json!({
                "document_id": document_id,
                "page_index": page_index,
                "offset": offset,
                "total_bytes": total_bytes,
                "text": text,
                "next_offset": next_offset,
            })),
            LibraryResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn capture_selection(
        &mut self,
        document_id: i64,
        page_index: u32,
        start: u32,
        end: u32,
        note: String,
        operation_id: String,
    ) -> Result<Value, String> {
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_7,
            request_id: operation_id,
            operation: LibraryOperation::CaptureSelection {
                document_id,
                page_index,
                start,
                end,
                note,
            },
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_library(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |context| Ok(bridge.respond(&request, context)))
            .map_err(text)?;
        match response.result {
            LibraryResult::Capture {
                capture_id,
                commit_rev,
            } => Ok(json!({
                "capture_id": capture_id,
                "commit_rev": commit_rev,
            })),
            LibraryResult::Page { .. } => Err("unexpected Library page result".into()),
            LibraryResult::PageText { .. } => Err("unexpected Library text result".into()),
            LibraryResult::SavedSelections { .. } => {
                Err("unexpected Library saved-selection result".into())
            }
            LibraryResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn saved_selections(&mut self, document_id: i64, page_index: u32) -> Result<Value, String> {
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_7,
            request_id: "workbench-library-saved-selections".into(),
            operation: LibraryOperation::SavedSelections {
                document_id,
                page_index,
            },
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_library(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |context| Ok(bridge.respond(&request, context)))
            .map_err(text)?;
        match response.result {
            LibraryResult::SavedSelections {
                document_id,
                page_index,
                ranges,
            } => Ok(json!({
                "document_id": document_id,
                "page_index": page_index,
                "ranges": ranges,
            })),
            LibraryResult::Page { .. } => Err("unexpected Library page result".into()),
            LibraryResult::PageText { .. } => Err("unexpected Library text result".into()),
            LibraryResult::Capture { .. } => Err("unexpected Library capture result".into()),
            LibraryResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn pdf_toc(&self, sys: &System, document_id: i64) -> Result<Value, String> {
        #[cfg(unix)]
        {
            crate::pdf_reader::extract_toc(sys, self.store.clone(), document_id)
        }
        #[cfg(not(unix))]
        {
            let _ = sys;
            Ok(json!({ "document_id": document_id, "toc": [] }))
        }
    }

    fn delete_document(&mut self, document_id: i64, format: &str) -> Result<Value, String> {
        let mut app = KnowledgeApplication::new(self.store.clone());
        match format {
            "pdf" => app
                .delete_pdf_document("qusor-vault", document_id)
                .map_err(text)?,
            "epub" => app
                .delete_epub_document("qusor-vault", document_id)
                .map_err(text)?,
            _ => return Err("unknown document format".into()),
        }
        Ok(json!({
            "deleted": true,
            "document_id": document_id,
            "format": format,
        }))
    }

    fn epub_page(&self, cursor: String, limit: u16) -> Result<Value, String> {
        let app = KnowledgeApplication::new(self.store.clone());
        let after = (!cursor.is_empty()).then_some(cursor.as_str());
        let page_limit = usize::from(limit).clamp(1, MAX_LIBRARY_PAGE.into());
        let page = app
            .epub_documents_page("qusor-vault", after, page_limit)
            .map_err(text)?;
        let items = page
            .documents
            .into_iter()
            .map(|doc| {
                json!({
                    "document_id": doc.document_id,
                    "commit_rev": doc.commit_rev,
                    "title": doc.title,
                    "author": doc.author,
                    "original_sha256": doc.original_sha256,
                    "original_bytes": doc.original_bytes,
                    "extraction_revision": doc.extraction_revision,
                    "chapter_count": doc.chapter_count,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "items": items,
            "next_cursor": page.next_cursor,
        }))
    }

    fn epub_toc(&self, document_id: i64) -> Result<Value, String> {
        let app = KnowledgeApplication::new(self.store.clone());
        let toc = app.epub_toc("qusor-vault", document_id).map_err(text)?;
        Ok(json!({
            "document_id": document_id,
            "toc": toc,
        }))
    }

    fn epub_chapter(&self, document_id: i64, chapter_index: u32) -> Result<Value, String> {
        let app = KnowledgeApplication::new(self.store.clone());
        let mut offset = 0usize;
        let mut title = String::new();
        let mut href = String::new();
        let mut full_text = String::new();
        loop {
            let chunk = app
                .epub_chapter_text(
                    "qusor-vault",
                    document_id,
                    chapter_index,
                    offset,
                    MAX_EPUB_TEXT_CHUNK,
                )
                .map_err(text)?;
            if offset == 0 {
                title = chunk.title;
                href = chunk.href;
                full_text.reserve(chunk.total_bytes.min(MAX_EPUB_CHAPTER_BYTES));
            }
            full_text.push_str(&chunk.text);
            if full_text.len() > MAX_EPUB_CHAPTER_BYTES {
                return Err("EPUB chapter text exceeds maximum read budget".into());
            }
            match chunk.next_offset {
                Some(next) if next > offset => offset = next,
                _ => break,
            }
        }
        Ok(json!({
            "document_id": document_id,
            "chapter_index": chapter_index,
            "title": title,
            "href": href,
            "text": full_text,
        }))
    }

    fn epub_saved_selections(&self, document_id: i64, chapter_index: u32) -> Result<Value, String> {
        let app = KnowledgeApplication::new(self.store.clone());
        let ranges = app
            .epub_saved_selections("qusor-vault", document_id, chapter_index)
            .map_err(text)?;
        let serialized = ranges
            .into_iter()
            .map(|r| {
                json!({
                    "capture_id": r.capture_id,
                    "start": r.start,
                    "end": r.end,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "document_id": document_id,
            "chapter_index": chapter_index,
            "ranges": serialized,
        }))
    }

    fn capture_epub_selection(
        &mut self,
        document_id: i64,
        chapter_index: u32,
        start: u32,
        end: u32,
        note: String,
        operation_id: String,
    ) -> Result<Value, String> {
        let mut app = KnowledgeApplication::new(self.store.clone());
        let receipt = app
            .capture_epub_selection(
                &operation_id,
                "qusor-vault",
                EpubSelectionRange {
                    document_id,
                    chapter_index,
                    start: u64::from(start),
                    end: u64::from(end),
                },
                &self.provenance,
                &note,
            )
            .map_err(text)?;
        Ok(json!({
            "capture_id": receipt.capture_id,
            "commit_rev": receipt.commit_rev,
        }))
    }
}

fn sanitize_line_text(input: &str, max_bytes: usize) -> String {
    let mut out = String::new();
    for ch in input.trim().chars() {
        let mapped = if ch.is_control() { ' ' } else { ch };
        if out.len() + mapped.len_utf8() > max_bytes {
            break;
        }
        out.push(mapped);
    }
    out.trim().to_string()
}

fn sanitize_chapter_text(input: &str, max_bytes: usize) -> String {
    let mut out = String::with_capacity(input.len().min(max_bytes));
    for ch in input.chars() {
        let mapped = if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t') {
            ' '
        } else {
            ch
        };
        if out.len() + mapped.len_utf8() > max_bytes {
            break;
        }
        out.push(mapped);
    }
    out
}

fn build_epub_snapshot(title: &str, bytes: &[u8]) -> Result<EpubSnapshot, String> {
    if bytes.is_empty() || bytes.len() > MAX_EPUB_BYTES {
        return Err("EPUB 文件为空或超过 16 MiB 限制".into());
    }
    let book = epub_reader::parse_epub(bytes)?;
    if book.chapters.is_empty() {
        return Err("EPUB 不包含可读章节内容".into());
    }
    let fallback_title = title
        .trim()
        .strip_suffix(".epub")
        .or_else(|| title.trim().strip_suffix(".EPUB"))
        .unwrap_or(title.trim());
    let resolved_title = book
        .title
        .as_deref()
        .map(|s| sanitize_line_text(s, MAX_EPUB_TITLE_BYTES))
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let cleaned = sanitize_line_text(fallback_title, MAX_EPUB_TITLE_BYTES);
            (!cleaned.is_empty()).then_some(cleaned)
        })
        .unwrap_or_else(|| "未命名书籍".to_string());
    let resolved_author = book
        .creator
        .as_deref()
        .map(|s| sanitize_line_text(s, MAX_EPUB_AUTHOR_BYTES))
        .unwrap_or_default();
    let chapters = book
        .chapters
        .into_iter()
        .take(MAX_EPUB_CHAPTERS)
        .enumerate()
        .map(|(idx, chapter)| {
            let ch_title = chapter
                .title
                .as_deref()
                .map(|s| sanitize_line_text(s, MAX_EPUB_TITLE_BYTES))
                .unwrap_or_default();
            let raw_href = sanitize_line_text(&chapter.href, MAX_EPUB_HREF_BYTES);
            let href = if raw_href.is_empty() {
                format!("chapter-{idx}.xhtml")
            } else {
                raw_href
            };
            EpubChapterSnapshot {
                chapter_index: idx as u32,
                title: ch_title,
                href,
                text: sanitize_chapter_text(&chapter.text, MAX_EPUB_CHAPTER_BYTES),
            }
        })
        .collect::<Vec<_>>();
    let chapter_count = chapters.len();
    let toc = book
        .toc
        .into_iter()
        .filter(|entry| entry.chapter_index < chapter_count)
        .filter_map(|entry| {
            let clean = sanitize_line_text(&entry.title, MAX_EPUB_TITLE_BYTES);
            if clean.is_empty() {
                return None;
            }
            Some(EpubTocEntry {
                title: clean,
                chapter_index: entry.chapter_index as u32,
                depth: entry.depth.min(MAX_EPUB_TOC_DEPTH),
            })
        })
        .take(MAX_EPUB_TOC_ENTRIES)
        .collect::<Vec<_>>();
    let snapshot = EpubSnapshot {
        vault: "qusor-vault".into(),
        title: resolved_title,
        author: resolved_author,
        original_sha256: sha256(bytes),
        original_bytes: bytes.len() as u64,
        extraction_revision: "qusor-epub-rs-v1".into(),
        chapters,
        toc,
    };
    snapshot.validate().map_err(text)?;
    Ok(snapshot)
}

struct EpubCompletion {
    value: Option<Result<String, String>>,
    waker: Option<Waker>,
}

struct EpubFuture {
    state: Arc<Mutex<EpubCompletion>>,
}

impl Future for EpubFuture {
    type Output = Result<String, String>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state.lock() {
            Ok(mut state) => match state.value.take() {
                Some(result) => Poll::Ready(result),
                None => {
                    state.waker = Some(context.waker().clone());
                    Poll::Pending
                }
            },
            Err(_) => Poll::Ready(Err("EPUB import state unavailable".into())),
        }
    }
}

fn import_epub_with_store(store: VaultStore, title: &str, bytes: &[u8]) -> Result<Value, String> {
    let snapshot = build_epub_snapshot(title, bytes)?;
    let op = format!(
        "library-epub-{}",
        &sha256(format!("{}\0{}", snapshot.title, snapshot.original_sha256).as_bytes())[..32]
    );
    let chapter_count = snapshot.chapters.len();
    let doc_title = snapshot.title.clone();
    let author = snapshot.author.clone();
    let receipt = KnowledgeApplication::new(store)
        .import_epub(&op, "qusor-vault", &snapshot)
        .map_err(text)?;
    Ok(json!({
        "document_id": receipt.document_id,
        "commit_rev": receipt.commit_rev,
        "title": doc_title,
        "author": author,
        "chapter_count": chapter_count,
    }))
}

fn import_epub_async(sys: &System, title: String, bytes: Vec<u8>) -> EpubFuture {
    let store = match runtime::with_library(sys, |library| Ok(library.store.clone())) {
        Ok(store) => store,
        Err(error) => {
            return EpubFuture {
                state: Arc::new(Mutex::new(EpubCompletion {
                    value: Some(Ok(json!({"ok": false, "error": error}).to_string())),
                    waker: None,
                })),
            };
        }
    };
    if bytes.is_empty() || bytes.len() > MAX_EPUB_BYTES {
        return EpubFuture {
            state: Arc::new(Mutex::new(EpubCompletion {
                value: Some(Ok(
                    json!({"ok": false, "error": "EPUB 文件为空或超过 16 MiB 限制"}).to_string(),
                )),
                waker: None,
            })),
        };
    }

    let state = Arc::new(Mutex::new(EpubCompletion {
        value: None,
        waker: None,
    }));
    let done = Arc::clone(&state);
    if let Err(err) = std::thread::Builder::new()
        .name("qusor-epub-import".into())
        .spawn(move || {
            let result = import_epub_with_store(store, &title, &bytes);
            let response = match result {
                Ok(value) => json!({"ok": true, "value": value}).to_string(),
                Err(error) => json!({"ok": false, "error": error}).to_string(),
            };
            if let Ok(mut slot) = done.lock() {
                slot.value = Some(Ok(response));
                if let Some(waker) = slot.waker.take() {
                    waker.wake();
                }
            }
        })
        && let Ok(mut slot) = state.lock()
    {
        slot.value = Some(Ok(json!({
            "ok": false,
            "error": format!("failed to spawn EPUB import thread: {err}")
        })
        .to_string()));
    }
    EpubFuture { state }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn wire_error(error: ApiError) -> String {
    format!("{:?}: {}", error.code, error.message)
}

/// Native Library surface for bounded metadata reads.
#[module]
pub struct LibraryHost;

#[module]
impl LibraryHost {
    /// Import one user-selected PDF into the local Core.
    pub fn import_pdf(sys: System, title: String, bytes: Vec<u8>) -> Later<String> {
        #[cfg(unix)]
        {
            Later::new(crate::pdf_service::import(sys.data_dir, title, bytes))
        }
        #[cfg(not(unix))]
        {
            let _ = (sys, title, bytes);
            Later::new(async {
                Ok(
                    json!({"ok": false, "error": "PDF import is not available on this platform yet"})
                        .to_string(),
                )
            })
        }
    }

    /// Render one page from the retained original as a native image.
    pub fn page_image(
        sys: System,
        document_id: i64,
        page_index: u32,
    ) -> Later<crate::pdf_page::PdfPageFile> {
        #[cfg(unix)]
        {
            Later::new(crate::pdf_service::render(
                sys.data_dir,
                document_id,
                page_index,
            ))
        }
        #[cfg(not(unix))]
        {
            let _ = (sys, document_id, page_index);
            Later::new(async {
                Ok(crate::pdf_page::failed_page(
                    "PDF page rendering is not available on this platform yet".into(),
                ))
            })
        }
    }

    /// Render a page into the bounded native cache without crossing an image.
    pub fn prefetch_page(sys: System, document_id: i64, page_index: u32) -> Later<String> {
        #[cfg(unix)]
        {
            Later::new(crate::pdf_service::prefetch(
                sys.data_dir,
                document_id,
                page_index,
            ))
        }
        #[cfg(not(unix))]
        {
            let _ = (sys, document_id, page_index);
            Later::new(async {
                Ok(
                    json!({"ok": false, "error": "PDF prefetch is not available on this platform"})
                        .to_string(),
                )
            })
        }
    }

    /// One newest-first page of PDF metadata in the current vault.
    pub fn page(sys: System, cursor: String, limit: u16) -> String {
        let cursor = if cursor.is_empty() {
            None
        } else {
            Some(cursor)
        };
        match runtime::with_library(&sys, |library| library.page(cursor, limit)) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// One bounded text-layer chunk for a Library document page.
    pub fn page_text(
        sys: System,
        document_id: i64,
        page_index: u32,
        offset: u32,
        max_bytes: u16,
    ) -> String {
        match runtime::with_library(&sys, |library| {
            library.page_text(document_id, page_index, offset, max_bytes)
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Saved scalar ranges on one PDF page for page-image highlights.
    pub fn saved_selections(sys: System, document_id: i64, page_index: u32) -> String {
        match runtime::with_library(&sys, |library| {
            library.saved_selections(document_id, page_index)
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Capture the current scalar range through the trusted Core adapter.
    pub fn capture_selection(
        sys: System,
        document_id: i64,
        page_index: u32,
        start: u32,
        end: u32,
        note: String,
        operation_id: String,
    ) -> String {
        match runtime::with_library(&sys, |library| {
            library.capture_selection(document_id, page_index, start, end, note, operation_id)
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Return cached or extracted bookmark TOC entries for one imported PDF document.
    pub fn pdf_toc(sys: System, document_id: i64) -> String {
        match runtime::with_library(&sys, |library| library.pdf_toc(&sys, document_id)) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Delete a PDF or EPUB document and its saved captures from Core.
    pub fn delete_document(sys: System, document_id: i64, format: String) -> String {
        match runtime::with_library(&sys, |library| {
            library.delete_document(document_id, &format)
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Import a user-selected EPUB book into the local Core in a background worker thread.
    pub fn import_epub(sys: System, title: String, bytes: Vec<u8>) -> Later<String> {
        Later::new(import_epub_async(&sys, title, bytes))
    }

    /// One newest-first page of EPUB metadata in the current vault.
    pub fn epub_page(sys: System, cursor: String, limit: u16) -> String {
        match runtime::with_library(&sys, |library| library.epub_page(cursor, limit)) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Table of contents entries for an imported EPUB book.
    pub fn epub_toc(sys: System, document_id: i64) -> String {
        match runtime::with_library(&sys, |library| library.epub_toc(document_id)) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Full extracted plain text and title of one EPUB chapter.
    pub fn epub_chapter(sys: System, document_id: i64, chapter_index: u32) -> String {
        match runtime::with_library(&sys, |library| {
            library.epub_chapter(document_id, chapter_index)
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Saved scalar ranges in one EPUB chapter for highlights.
    pub fn epub_saved_selections(sys: System, document_id: i64, chapter_index: u32) -> String {
        match runtime::with_library(&sys, |library| {
            library.epub_saved_selections(document_id, chapter_index)
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Capture the selected scalar range in an EPUB chapter into Core.
    pub fn capture_epub_selection(
        sys: System,
        document_id: i64,
        chapter_index: u32,
        start: u32,
        end: u32,
        note: String,
        operation_id: String,
    ) -> String {
        match runtime::with_library(&sys, |library| {
            library.capture_epub_selection(
                document_id,
                chapter_index,
                start,
                end,
                note,
                operation_id,
            )
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes_native::NotesRuntime;
    use std::{
        fs,
        io::{Cursor, Write},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    use zip::{ZipWriter, write::SimpleFileOptions};

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "qusor-library-native-test-{}-{}",
                std::process::id(),
                NEXT_TEST.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample_epub_bytes() -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            let options = SimpleFileOptions::default();
            let entries = [
                (
                    "META-INF/container.xml",
                    r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
                ),
                (
                    "OEBPS/content.opf",
                    r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Sample EPUB Book</dc:title>
    <dc:creator>Ada</dc:creator>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
  </spine>
</package>"#,
                ),
                (
                    "OEBPS/nav.xhtml",
                    r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body>
  <nav epub:type="toc"><ol><li><a href="ch1.xhtml">Chapter One</a></li></ol></nav>
</body>
</html>"#,
                ),
                (
                    "OEBPS/ch1.xhtml",
                    r#"<html><body><h1>Chapter One</h1><p>First EPUB quote for notes.</p></body></html>"#,
                ),
            ];
            for (name, body) in entries {
                zip.start_file(name, options).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn epub_import_read_capture_notes_and_delete_roundtrip() {
        let scratch = Scratch::new();
        let store = VaultStore::open_app(&scratch.0.join("data")).unwrap();

        let imported =
            import_epub_with_store(store.clone(), "sample.epub", &sample_epub_bytes()).unwrap();
        let document_id = imported["document_id"].as_i64().unwrap();
        assert_eq!(imported["title"], "Sample EPUB Book");
        assert_eq!(imported["author"], "Ada");
        assert_eq!(imported["chapter_count"], 1);

        let mut library = LibraryRuntime::mount(store.clone()).unwrap();
        let page = library.epub_page(String::new(), 10).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        assert_eq!(page["items"][0]["document_id"], document_id);

        let toc = library.epub_toc(document_id).unwrap();
        assert_eq!(toc["toc"].as_array().unwrap().len(), 1);
        assert_eq!(toc["toc"][0]["title"], "Chapter One");

        let chapter = library.epub_chapter(document_id, 0).unwrap();
        let text = chapter["text"].as_str().unwrap();
        assert!(text.contains("First EPUB quote for notes."));

        let start = text.find("First EPUB quote").unwrap();
        let scalar_start = text[..start].chars().count() as u32;
        let scalar_end = scalar_start + "First EPUB quote".chars().count() as u32;
        let captured = library
            .capture_epub_selection(
                document_id,
                0,
                scalar_start,
                scalar_end,
                "Initial EPUB note".into(),
                "epub-cap-1".into(),
            )
            .unwrap();
        let capture_id = captured["capture_id"].as_i64().unwrap();

        let selections = library.epub_saved_selections(document_id, 0).unwrap();
        assert_eq!(selections["ranges"].as_array().unwrap().len(), 1);
        assert_eq!(selections["ranges"][0]["capture_id"], capture_id);
        assert_eq!(selections["ranges"][0]["start"], scalar_start);
        assert_eq!(selections["ranges"][0]["end"], scalar_end);

        let mut notes = NotesRuntime::mount(store).unwrap();
        let epub_notes = notes.epub_notes_page(String::new(), 10).unwrap();
        assert_eq!(epub_notes["notes"].as_array().unwrap().len(), 1);
        assert_eq!(epub_notes["notes"][0]["capture_id"], capture_id);
        assert_eq!(epub_notes["notes"][0]["note_preview"], "Initial EPUB note");

        let updated = notes
            .update_note(capture_id, "epub".into(), "Edited EPUB note".into())
            .unwrap();
        assert_eq!(updated["updated"], true);

        let body = notes
            .epub_body(capture_id, "note".into(), 0, 1024)
            .unwrap();
        assert_eq!(body["text"], "Edited EPUB note");

        let deleted_note = notes.delete_note(capture_id, "epub".into()).unwrap();
        assert_eq!(deleted_note["deleted"], true);
        assert!(
            notes
                .epub_notes_page(String::new(), 10)
                .unwrap()["notes"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let deleted_doc = library.delete_document(document_id, "epub").unwrap();
        assert_eq!(deleted_doc["deleted"], true);
        assert!(
            library
                .epub_page(String::new(), 10)
                .unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
