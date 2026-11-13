//! One process-lifetime serial native service for bounded PDF work.
//!
//! PDFium initializes process-global bindings and can spend seconds on a real
//! page. Requests therefore run on one dedicated thread, never on the UI
//! thread. This is resource isolation, not crash isolation.

use crate::pdf_page::{PdfPageData, PdfPageFile};
use nsh_core::element::System;
use serde_json::json;
use std::{
    collections::VecDeque,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Mutex, OnceLock,
        mpsc::{self, Sender},
    },
    task::{Context, Poll, Waker},
    thread,
};

const MAX_CACHE_PAGES: usize = 3;
const MAX_CACHE_BYTES: usize = 32 * 1024 * 1024;

struct CachedPage {
    document_id: i64,
    page_index: u32,
    data: PdfPageData,
    encoded_bytes: usize,
}

struct PageCache {
    entries: VecDeque<CachedPage>,
    encoded_bytes: usize,
}

impl PageCache {
    fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            encoded_bytes: 0,
        }
    }

    fn get(&mut self, document_id: i64, page_index: u32) -> Option<PdfPageData> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.document_id == document_id && entry.page_index == page_index)?;
        let entry = self.entries.remove(index)?;
        let data = entry.data.clone();
        self.entries.push_back(entry);
        Some(data)
    }

    fn insert(&mut self, document_id: i64, page_index: u32, data: PdfPageData) {
        let encoded_bytes = data.image.as_ref().map_or(0, |image| image.len());
        if encoded_bytes > MAX_CACHE_BYTES {
            return;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.document_id == document_id && entry.page_index == page_index)
            && let Some(entry) = self.entries.remove(index)
        {
            self.encoded_bytes = self.encoded_bytes.saturating_sub(entry.encoded_bytes);
        }
        self.entries.push_back(CachedPage {
            document_id,
            page_index,
            data,
            encoded_bytes,
        });
        self.encoded_bytes += encoded_bytes;
        while self.entries.len() > MAX_CACHE_PAGES
            || (self.encoded_bytes > MAX_CACHE_BYTES && self.entries.len() > 1)
        {
            if let Some(entry) = self.entries.pop_front() {
                self.encoded_bytes = self.encoded_bytes.saturating_sub(entry.encoded_bytes);
            }
        }
    }
}

struct Completion<T> {
    value: Option<Result<T, String>>,
    waker: Option<Waker>,
}

pub(crate) struct PdfFuture<T> {
    state: Arc<Mutex<Completion<T>>>,
}

impl<T> Future for PdfFuture<T> {
    type Output = Result<T, String>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state.lock() {
            Ok(mut state) => match state.value.take() {
                Some(result) => Poll::Ready(result),
                None => {
                    state.waker = Some(context.waker().clone());
                    Poll::Pending
                }
            },
            Err(_) => Poll::Ready(Err("the PDF service completion is unavailable".into())),
        }
    }
}

enum PdfJob {
    Import {
        data_dir: Option<PathBuf>,
        title: String,
        bytes: Vec<u8>,
        done: Arc<Mutex<Completion<String>>>,
    },
    Render {
        data_dir: Option<PathBuf>,
        document_id: i64,
        page_index: u32,
        done: Arc<Mutex<Completion<PdfPageFile>>>,
    },
    Prefetch {
        data_dir: Option<PathBuf>,
        document_id: i64,
        page_index: u32,
        done: Arc<Mutex<Completion<String>>>,
    },
}

static SERVICE: OnceLock<Result<Sender<PdfJob>, String>> = OnceLock::new();

fn sender() -> Result<Sender<PdfJob>, String> {
    SERVICE
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<PdfJob>();
            thread::Builder::new()
                .name("qusor-pdf-service".into())
                .spawn(move || {
                    let mut cache = PageCache::new();
                    while let Ok(job) = receiver.recv() {
                        run(job, &mut cache);
                    }
                })
                .map(|_| sender)
                .map_err(|error| format!("could not start the PDF service: {error}"))
        })
        .clone()
}

fn worker_system(data_dir: PathBuf) -> System {
    let mut system = System::default();
    system.data_dir = Some(data_dir);
    system
}

fn complete<T>(done: &Arc<Mutex<Completion<T>>>, result: Result<T, String>) {
    let waker = match done.lock() {
        Ok(mut state) => {
            state.value = Some(result);
            state.waker.take()
        }
        Err(_) => None,
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}

fn render_page(
    cache: &mut PageCache,
    data_dir: Option<PathBuf>,
    document_id: i64,
    page_index: u32,
) -> Result<PdfPageData, String> {
    if let Some(page) = cache.get(document_id, page_index) {
        return Ok(page);
    }
    let data_dir = data_dir.ok_or("the host gave no app data directory")?;
    let system = worker_system(data_dir);
    let page = crate::runtime::with_library(&system, |library| {
        library.render_pdf_page(&system, document_id, page_index)
    })?;
    cache.insert(document_id, page_index, page.clone());
    Ok(page)
}

fn run(job: PdfJob, cache: &mut PageCache) {
    match job {
        PdfJob::Import {
            data_dir,
            title,
            bytes,
            done,
        } => {
            let result = (|| {
                let data_dir = data_dir.ok_or("the host gave no app data directory")?;
                let system = worker_system(data_dir);
                crate::runtime::with_library(&system, |library| {
                    library.import_pdf(&system, title, bytes)
                })
            })();
            complete(
                &done,
                Ok(match result {
                    Ok(value) => json!({"ok": true, "value": value}).to_string(),
                    Err(error) => json!({"ok": false, "error": error}).to_string(),
                }),
            );
        }
        PdfJob::Render {
            data_dir,
            document_id,
            page_index,
            done,
        } => {
            let result = render_page(cache, data_dir, document_id, page_index);
            complete(
                &done,
                Ok(match result {
                    Ok(page) => PdfPageFile::from(page),
                    Err(error) => crate::pdf_page::failed_page(error),
                }),
            );
        }
        PdfJob::Prefetch {
            data_dir,
            document_id,
            page_index,
            done,
        } => {
            let result = render_page(cache, data_dir, document_id, page_index);
            complete(
                &done,
                Ok(match result {
                    Ok(_) => json!({"ok": true}).to_string(),
                    Err(error) => json!({"ok": false, "error": error}).to_string(),
                }),
            );
        }
    }
}

fn rejected<T>(message: String) -> PdfFuture<T> {
    PdfFuture {
        state: Arc::new(Mutex::new(Completion {
            value: Some(Err(message)),
            waker: None,
        })),
    }
}

pub(crate) fn import(
    data_dir: Option<PathBuf>,
    title: String,
    bytes: Vec<u8>,
) -> PdfFuture<String> {
    let done = Arc::new(Mutex::new(Completion {
        value: None,
        waker: None,
    }));
    let job = PdfJob::Import {
        data_dir,
        title,
        bytes,
        done: done.clone(),
    };
    if let Err(error) = sender().and_then(|sender| {
        sender
            .send(job)
            .map_err(|_| "the PDF service stopped".to_string())
    }) {
        return rejected(error);
    }
    PdfFuture { state: done }
}

pub(crate) fn render(
    data_dir: Option<PathBuf>,
    document_id: i64,
    page_index: u32,
) -> PdfFuture<PdfPageFile> {
    let done = Arc::new(Mutex::new(Completion {
        value: None,
        waker: None,
    }));
    let job = PdfJob::Render {
        data_dir,
        document_id,
        page_index,
        done: done.clone(),
    };
    if let Err(error) = sender().and_then(|sender| {
        sender
            .send(job)
            .map_err(|_| "the PDF service stopped".to_string())
    }) {
        return rejected(error);
    }
    PdfFuture { state: done }
}

pub(crate) fn prefetch(
    data_dir: Option<PathBuf>,
    document_id: i64,
    page_index: u32,
) -> PdfFuture<String> {
    let done = Arc::new(Mutex::new(Completion {
        value: None,
        waker: None,
    }));
    let job = PdfJob::Prefetch {
        data_dir,
        document_id,
        page_index,
        done: done.clone(),
    };
    if let Err(error) = sender().and_then(|sender| {
        sender
            .send(job)
            .map_err(|_| "the PDF service stopped".to_string())
    }) {
        return rejected(error);
    }
    PdfFuture { state: done }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsh_core::ImageData;

    fn page(bytes: usize) -> PdfPageData {
        PdfPageData {
            info: "{}".into(),
            image: Some(ImageData::new(vec![0; bytes])),
            geometry: String::new(),
        }
    }

    #[test]
    fn cache_is_lru_and_bounded_by_page_count() {
        let mut cache = PageCache::new();
        for index in 0..MAX_CACHE_PAGES as u32 {
            cache.insert(1, index, page(4));
        }
        assert!(cache.get(1, 0).is_some());
        cache.insert(1, 3, page(4));
        assert!(cache.get(1, 1).is_none());
        assert!(cache.get(1, 0).is_some());
        assert!(cache.get(1, 3).is_some());
    }

    #[test]
    fn oversized_page_is_not_cached() {
        let mut cache = PageCache::new();
        cache.insert(1, 0, page(MAX_CACHE_BYTES + 1));
        assert!(cache.get(1, 0).is_none());
    }
}
