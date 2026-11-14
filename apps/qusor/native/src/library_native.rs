//! Trusted Library Host mounted by the application composition runtime.
use crate::{core_facade, epub_reader, package_catalog, profile, runtime, vault::VaultStore};
use core_api::{
    contract::sha256,
    epub_contract::{
        EpubChapterSnapshot, EpubSnapshot, EpubTocEntry, MAX_EPUB_AUTHOR_BYTES, MAX_EPUB_BYTES,
        MAX_EPUB_CHAPTER_BYTES, MAX_EPUB_CHAPTERS, MAX_EPUB_HREF_BYTES, MAX_EPUB_TITLE_BYTES,
        MAX_EPUB_TOC_DEPTH, MAX_EPUB_TOC_ENTRIES,
    },
};
use extension_api::{
    ApiError, Capability, ErrorCode, Resource, SUPPORTED_HOST_APIS,
    library::*,
    orthogonal::{CapabilityId, STANDARD_ORTHOGONAL_CAPABILITIES},
};
use extension_host::{Grant, Host, OrthogonalGrant, Session};
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
}

impl LibraryRuntime {
    pub(crate) fn mount_for_package(store: VaultStore, package_id: &str) -> Result<Self, String> {
        let verified = package_catalog::verify(package_id)?;
        Self::mount_verified(store, verified)
    }

    fn mount_verified(
        store: VaultStore,
        verified: extension_package::package::VerifiedPackage,
    ) -> Result<Self, String> {
        profile::require_service_active(profile::LIBRARY_READ_SERVICE)?;
        let vault_id = store.vault_id();
        let mut host = Host::new(
            vault_id,
            [
                Capability::LibraryRead,
                Capability::CapturePropose,
                Capability::UiSlot,
            ],
        )
        .map_err(text)?;
        for cap in STANDARD_ORTHOGONAL_CAPABILITIES {
            let cap_id =
                CapabilityId::parse_with_code(cap, ErrorCode::InvalidRequest).map_err(text)?;
            host.configure_orthogonal_support(cap_id, 1, 0)
                .map_err(text)?;
        }
        let installation = host
            .register(verified.manifest().clone(), verified.digest())
            .map_err(text)?;
        let declared_grants: Vec<Grant> = [
            Capability::LibraryRead,
            Capability::CapturePropose,
            Capability::UiSlot,
        ]
        .into_iter()
        .filter(|cap| {
            verified.manifest().required_capabilities.contains(cap)
                || verified.manifest().optional_capabilities.contains(cap)
        })
        .map(|capability| Grant {
            capability,
            resource: Resource::Vault {
                vault_id: vault_id.to_owned(),
            },
        })
        .collect();
        host.replace_grants(installation, declared_grants)
            .map_err(text)?;
        let orthogonal_grants: Vec<OrthogonalGrant> = verified
            .manifest()
            .effective_orthogonal_requirements()
            .into_iter()
            .map(|req| OrthogonalGrant {
                capability: req.id,
                min_minor: req.min_minor,
                resource: Resource::Vault {
                    vault_id: vault_id.to_owned(),
                },
            })
            .collect();
        host.replace_orthogonal_grants(installation, orthogonal_grants)
            .map_err(text)?;
        host.enable(installation).map_err(text)?;
        let session = host
            .open_negotiated_session(installation, &SUPPORTED_HOST_APIS)
            .map_err(text)?;
        let bridge = core_facade::new_library_bridge(&store);
        Ok(Self {
            host,
            session,
            store,
            bridge,
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
        let api_version = self.host.session_api(&self.session).map_err(text)?;
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version,
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
        let api_version = self.host.session_api(&self.session).map_err(text)?;
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version,
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
        let api_version = self.host.session_api(&self.session).map_err(text)?;
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version,
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
        let api_version = self.host.session_api(&self.session).map_err(text)?;
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version,
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

fn build_epub_snapshot(vault_id: &str, title: &str, bytes: &[u8]) -> Result<EpubSnapshot, String> {
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
        vault: vault_id.into(),
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
    let snapshot = build_epub_snapshot(store.vault_id(), title, bytes)?;
    let op = format!(
        "library-epub-{}",
        &sha256(format!("{}\0{}", snapshot.title, snapshot.original_sha256).as_bytes())[..32]
    );
    let chapter_count = snapshot.chapters.len();
    let doc_title = snapshot.title.clone();
    let author = snapshot.author.clone();
    let receipt = core_facade::import_epub_document(&store, &op, &snapshot).map_err(text)?;
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

    /// Import a user-selected EPUB book into the local Core in a background worker thread.
    pub fn import_epub(sys: System, title: String, bytes: Vec<u8>) -> Later<String> {
        Later::new(import_epub_async(&sys, title, bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes_native::NotesRuntime;
    use extension_api::{
        HOST_API_V0_8,
        notes::NoteField,
        orthogonal::{
            CORE_KNOWLEDGE_PROTOCOL, CoreKnowledgeOperation, CoreKnowledgeRequest,
            CoreKnowledgeResult, DocumentFormat,
        },
    };
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

        let mut library =
            LibraryRuntime::mount_for_package(store.clone(), "org.qusor.library").unwrap();
        let mut ortho = core_facade::new_orthogonal_bridge(&store);
        let provenance = core_api::knowledge::Provenance {
            package_id: "org.qusor.library".into(),
            publisher_id: "org.qusor".into(),
            package_digest: "a".repeat(64),
            installation_id: 1,
        };
        let vault_resource = Resource::Vault {
            vault_id: store.vault_id().to_owned(),
        };
        let mut call_lib = |req_id: &str, operation: CoreKnowledgeOperation| {
            let req = CoreKnowledgeRequest {
                protocol_version: CORE_KNOWLEDGE_PROTOCOL,
                api_version: HOST_API_V0_8,
                request_id: req_id.into(),
                operation,
            };
            let permit = library
                .host
                .authorize_core_knowledge(&library.session, &req, vault_resource.clone())
                .unwrap();
            library
                .host
                .dispatch_orthogonal(permit, |_ctx| Ok(ortho.respond(&req, &provenance)))
                .unwrap()
                .result
        };

        let CoreKnowledgeResult::DocumentsPage { documents, .. } = call_lib(
            "lib-1",
            CoreKnowledgeOperation::DocumentsPage {
                format: Some(DocumentFormat::Epub),
                cursor: None,
                limit: 10,
            },
        ) else {
            panic!("expected DocumentsPage");
        };
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].document_id, document_id);

        let CoreKnowledgeResult::DocumentToc { entries, .. } = call_lib(
            "lib-2",
            CoreKnowledgeOperation::DocumentToc {
                format: DocumentFormat::Epub,
                document_id,
            },
        ) else {
            panic!("expected DocumentToc");
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Chapter One");

        let CoreKnowledgeResult::DocumentChunk { text, .. } = call_lib(
            "lib-3",
            CoreKnowledgeOperation::DocumentChunk {
                format: DocumentFormat::Epub,
                document_id,
                section_index: 0,
                offset: 0,
                max_bytes: 2048,
            },
        ) else {
            panic!("expected DocumentChunk");
        };
        assert!(text.contains("First EPUB quote for notes."));

        let start = text.find("First EPUB quote").unwrap();
        let scalar_start = text[..start].chars().count() as u32;
        let scalar_end = scalar_start + "First EPUB quote".chars().count() as u32;
        let CoreKnowledgeResult::CaptureCreated { capture_id, .. } = call_lib(
            "lib-4",
            CoreKnowledgeOperation::CaptureEpubSelection {
                operation_id: "epub-cap-1".into(),
                document_id,
                chapter_index: 0,
                start: scalar_start,
                end: scalar_end,
                quote: "First EPUB quote".into(),
                note: "Initial EPUB note".into(),
            },
        ) else {
            panic!("expected CaptureCreated");
        };

        let CoreKnowledgeResult::SavedSelections { ranges, .. } = call_lib(
            "lib-5",
            CoreKnowledgeOperation::SavedSelections {
                format: DocumentFormat::Epub,
                document_id,
                section_index: 0,
            },
        ) else {
            panic!("expected SavedSelections");
        };
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].capture_id, capture_id);
        assert_eq!(ranges[0].start, scalar_start);
        assert_eq!(ranges[0].end, scalar_end);

        let _notes = NotesRuntime::mount_for_package(store, "org.qusor.notes").unwrap();
        let CoreKnowledgeResult::NoteUpdated { .. } = call_lib(
            "lib-6",
            CoreKnowledgeOperation::UpdateCaptureNote {
                format: DocumentFormat::Epub,
                capture_id,
                note: "Edited EPUB note".into(),
            },
        ) else {
            panic!("expected NoteUpdated");
        };
        let CoreKnowledgeResult::NoteBody(body) = call_lib(
            "lib-7",
            CoreKnowledgeOperation::NoteBody {
                format: DocumentFormat::Epub,
                capture_id,
                field: NoteField::Note,
                offset: 0,
                max_bytes: 1024,
            },
        ) else {
            panic!("expected NoteBody");
        };
        assert_eq!(body.text, "Edited EPUB note");

        let CoreKnowledgeResult::CaptureDeleted { .. } = call_lib(
            "lib-8",
            CoreKnowledgeOperation::DeleteCapture {
                format: DocumentFormat::Epub,
                capture_id,
            },
        ) else {
            panic!("expected CaptureDeleted");
        };
        let CoreKnowledgeResult::DocumentDeleted { .. } = call_lib(
            "lib-9",
            CoreKnowledgeOperation::DeleteDocument {
                format: DocumentFormat::Epub,
                document_id,
            },
        ) else {
            panic!("expected DocumentDeleted");
        };
    }
}
