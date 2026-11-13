//! One process-lifetime serial native service for bounded PDF work.
//!
//! PDFium initializes process-global bindings and can spend seconds on a real
//! page. Requests therefore run on one dedicated thread, never on the UI
//! thread. This is resource isolation, not crash isolation.

use crate::pdf_page::PdfPageFile;
use nsh_core::element::System;
use serde_json::json;
use std::{
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
}

static SERVICE: OnceLock<Result<Sender<PdfJob>, String>> = OnceLock::new();

fn sender() -> Result<Sender<PdfJob>, String> {
    SERVICE
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<PdfJob>();
            thread::Builder::new()
                .name("qusor-pdf-service".into())
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        run(job);
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

fn run(job: PdfJob) {
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
            let result = (|| {
                let data_dir = data_dir.ok_or("the host gave no app data directory")?;
                let system = worker_system(data_dir);
                crate::runtime::with_library(&system, |library| {
                    library.render_pdf_page(&system, document_id, page_index)
                })
            })();
            complete(
                &done,
                Ok(match result {
                    Ok(page) => PdfPageFile::from(page),
                    Err(error) => crate::pdf_page::failed_page(error),
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
