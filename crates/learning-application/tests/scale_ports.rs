use core_api::{
    contract::{CaptureDraft, sha256},
    epub_contract::EpubSnapshot,
    error::{KnowledgeError, KnowledgeErrorCode},
    knowledge::{
        ArchiveV2, CaptureRecord, CommitEntry, EpubDocumentReceipt, EpubReceipt, EpubTocRecord,
        PdfCaptureRecord, PdfDocumentReceipt, PdfDocumentRecord, PdfReceipt, Provenance, Receipt,
        Result,
    },
    pdf_compound::PdfCompoundDraft,
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
    vocab::{
        CardSchedule, NewWord, ReviewEntry, ReviewGuard, StudyArchive, StudyCard, StudyRating,
        StudyStage, Wordbook, WordbookSeed,
    },
};
use learning_application::{
    KnowledgeApplication, annotation_source::FrozenPdfSource, pdf_notes::NoteBodyField,
    study::StudyApplication,
};
use learning_ports::{
    EpubCaptureWithVault, EpubChapterLookup, EpubDocumentsPageData, EpubNotesPageData,
    EpubSavedSelectionsLookup, KnowledgeArchive, KnowledgeCommand, KnowledgeQuery, OperationLookup,
    PdfCaptureWithVault, PdfDocumentSummaryRow, PdfDocumentsPageData, PdfNoteSummaryRow,
    PdfNotesPageData, PdfPageTextLookup, PdfSavedSelectionRaw, PdfSavedSelectionsLookup,
    StudyArchivePort, StudyCommand, StudyDeckCounts, StudyQuery, StudyQueueData, StudyQueueEntry,
    StudyReviewContext,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone)]
struct Counters {
    snapshot_calls: Arc<AtomicU64>,
    study_snapshot_calls: Arc<AtomicU64>,
    blob_reads: Arc<AtomicU64>,
}

impl Counters {
    fn new() -> Self {
        Self {
            snapshot_calls: Arc::new(AtomicU64::new(0)),
            study_snapshot_calls: Arc::new(AtomicU64::new(0)),
            blob_reads: Arc::new(AtomicU64::new(0)),
        }
    }
}

struct CountingSplitStore {
    counters: Counters,
    next_rev: i64,
    next_doc_id: i64,
    next_cap_id: i64,
    next_book_id: i64,
    next_card_id: i64,
    next_review_id: i64,
    pdf_docs: BTreeMap<i64, PdfDocumentRecord>,
    pdf_doc_ops: HashMap<String, PdfDocumentReceipt>,
    pdf_blobs: BTreeMap<i64, Vec<u8>>,
    pdf_captures: BTreeMap<i64, PdfCaptureRecord>,
    pdf_cap_ops: HashMap<String, i64>,
    commits: HashMap<String, CommitEntry>,
    wordbooks: BTreeMap<i64, Wordbook>,
    cards: BTreeMap<i64, StudyCard>,
    schedules: BTreeMap<i64, CardSchedule>,
    reviews: BTreeMap<i64, ReviewEntry>,
    review_ops: HashMap<String, i64>,
}

impl CountingSplitStore {
    fn new(counters: Counters) -> Self {
        Self {
            counters,
            next_rev: 0,
            next_doc_id: 0,
            next_cap_id: 0,
            next_book_id: 0,
            next_card_id: 0,
            next_review_id: 0,
            pdf_docs: BTreeMap::new(),
            pdf_doc_ops: HashMap::new(),
            pdf_blobs: BTreeMap::new(),
            pdf_captures: BTreeMap::new(),
            pdf_cap_ops: HashMap::new(),
            commits: HashMap::new(),
            wordbooks: BTreeMap::new(),
            cards: BTreeMap::new(),
            schedules: BTreeMap::new(),
            reviews: BTreeMap::new(),
            review_ops: HashMap::new(),
        }
    }
}

impl KnowledgeQuery for CountingSplitStore {
    fn text_capture(&self, _id: i64) -> Result<Option<CaptureRecord>> {
        Ok(None)
    }

    fn text_receipt_by_operation(&self, _op: &str) -> Result<Option<Receipt>> {
        Ok(None)
    }

    fn commit_by_operation(&self, op: &str) -> Result<Option<CommitEntry>> {
        Ok(self.commits.get(op).cloned())
    }

    fn pdf_document(&self, id: i64) -> Result<Option<PdfDocumentRecord>> {
        Ok(self.pdf_docs.get(&id).cloned())
    }

    fn pdf_document_receipt_by_operation(&self, op: &str) -> Result<Option<PdfDocumentReceipt>> {
        Ok(self.pdf_doc_ops.get(op).cloned())
    }

    fn pdf_capture(&self, id: i64) -> Result<Option<PdfCaptureRecord>> {
        Ok(self.pdf_captures.get(&id).cloned())
    }

    fn pdf_capture_with_vault(&self, id: i64) -> Result<Option<PdfCaptureWithVault>> {
        let Some(record) = self.pdf_captures.get(&id).cloned() else {
            return Ok(None);
        };
        let Some(doc) = self.pdf_docs.get(&record.receipt.document_id) else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::Integrity,
                "missing owning PDF document",
            ));
        };
        Ok(Some(PdfCaptureWithVault {
            record,
            document_vault: doc.vault.clone(),
        }))
    }

    fn pdf_capture_by_operation(&self, op: &str) -> Result<OperationLookup<PdfCaptureWithVault>> {
        if let Some(cap_id) = self.pdf_cap_ops.get(op) {
            return match self.pdf_capture_with_vault(*cap_id)? {
                Some(v) => Ok(OperationLookup::Match(v)),
                None => Ok(OperationLookup::Absent),
            };
        }
        if self.commits.contains_key(op) {
            return Ok(OperationLookup::OtherOperationKind);
        }
        Ok(OperationLookup::Absent)
    }

    fn pdf_documents_page(
        &self,
        vault: &str,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<PdfDocumentsPageData> {
        let mut rows: Vec<_> = self
            .pdf_docs
            .values()
            .filter(|d| d.vault == vault)
            .map(|d| PdfDocumentSummaryRow {
                document_id: d.receipt.document_id,
                commit_rev: d.receipt.commit_rev,
                title: d.title.clone(),
                original_sha256: d.receipt.original_sha256.clone(),
                original_bytes: self
                    .pdf_blobs
                    .get(&d.receipt.document_id)
                    .map(|b| b.len() as u64)
                    .unwrap_or(0),
                extraction_revision: d.receipt.extraction_revision.clone(),
                page_count: d.snapshot.pages.len(),
            })
            .collect();
        rows.sort_by_key(|b| std::cmp::Reverse((b.commit_rev, b.document_id)));
        if let Some(cursor) = after {
            rows.retain(|r| (r.commit_rev, r.document_id) < cursor);
        }
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        let next_cursor_key = if has_more {
            rows.last().map(|r| (r.commit_rev, r.document_id))
        } else {
            None
        };
        Ok(PdfDocumentsPageData {
            documents: rows,
            next_cursor_key,
        })
    }

    fn pdf_page_text(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Result<PdfPageTextLookup> {
        let Some(doc) = self.pdf_docs.get(&document_id).filter(|d| d.vault == vault) else {
            return Ok(PdfPageTextLookup::DocumentMissing);
        };
        let Some(page) = doc.snapshot.pages.get(page_index as usize) else {
            return Ok(PdfPageTextLookup::PageMissing);
        };
        Ok(PdfPageTextLookup::Found {
            engine_text: page.engine_text.clone(),
        })
    }

    fn pdf_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Result<PdfSavedSelectionsLookup> {
        let Some(doc) = self.pdf_docs.get(&document_id).filter(|d| d.vault == vault) else {
            return Ok(PdfSavedSelectionsLookup::DocumentMissing);
        };
        let Some(page) = doc.snapshot.pages.get(page_index as usize) else {
            return Ok(PdfSavedSelectionsLookup::PageMissing);
        };
        let mut ranges = Vec::new();
        for cap in self.pdf_captures.values() {
            if cap.receipt.document_id == document_id && cap.draft.page_index == page_index {
                ranges.push(PdfSavedSelectionRaw {
                    capture_id: cap.receipt.capture_id,
                    start: cap.draft.start,
                    end: cap.draft.end,
                });
            }
        }
        Ok(PdfSavedSelectionsLookup::Found {
            page_scalar_count: page.engine_text.chars().count(),
            ranges,
        })
    }

    fn pdf_notes_page(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<PdfNotesPageData> {
        let mut rows: Vec<_> = self
            .pdf_captures
            .values()
            .filter_map(|cap| {
                let doc = self.pdf_docs.get(&cap.receipt.document_id)?;
                if doc.vault != vault {
                    return None;
                }
                if let Some(target) = document_id
                    && doc.receipt.document_id != target
                {
                    return None;
                }
                Some(PdfNoteSummaryRow {
                    record: cap.clone(),
                    document_title: doc.title.clone(),
                })
            })
            .collect();
        rows.sort_by(|a, b| {
            (b.record.receipt.commit_rev, b.record.receipt.capture_id)
                .cmp(&(a.record.receipt.commit_rev, a.record.receipt.capture_id))
        });
        if let Some(cursor) = after {
            rows.retain(|r| (r.record.receipt.commit_rev, r.record.receipt.capture_id) < cursor);
        }
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        let next_cursor_key = if has_more {
            rows.last()
                .map(|r| (r.record.receipt.commit_rev, r.record.receipt.capture_id))
        } else {
            None
        };
        Ok(PdfNotesPageData {
            notes: rows,
            next_cursor_key,
        })
    }

    fn read_original_pdf(&self, id: i64) -> Result<Vec<u8>> {
        self.counters.blob_reads.fetch_add(1, Ordering::Relaxed);
        self.pdf_blobs.get(&id).cloned().ok_or_else(|| {
            KnowledgeError::new(KnowledgeErrorCode::NotFound, "missing PDF original blob")
        })
    }

    fn epub_documents_page(
        &self,
        _vault: &str,
        _after: Option<(i64, i64)>,
        _limit: usize,
    ) -> Result<EpubDocumentsPageData> {
        Ok(EpubDocumentsPageData {
            documents: vec![],
            next_cursor_key: None,
        })
    }

    fn epub_chapter_text(
        &self,
        _vault: &str,
        _document_id: i64,
        _chapter_index: u32,
    ) -> Result<EpubChapterLookup> {
        Ok(EpubChapterLookup::DocumentMissing)
    }

    fn epub_toc(&self, _vault: &str, _document_id: i64) -> Result<Option<Vec<EpubTocRecord>>> {
        Ok(None)
    }

    fn epub_saved_selections(
        &self,
        _vault: &str,
        _document_id: i64,
        _chapter_index: u32,
    ) -> Result<EpubSavedSelectionsLookup> {
        Ok(EpubSavedSelectionsLookup::DocumentMissing)
    }

    fn epub_notes_page(
        &self,
        _vault: &str,
        _document_id: Option<i64>,
        _after: Option<(i64, i64)>,
        _limit: usize,
    ) -> Result<EpubNotesPageData> {
        Ok(EpubNotesPageData {
            notes: vec![],
            next_cursor_key: None,
        })
    }

    fn epub_capture_with_vault(&self, _id: i64) -> Result<Option<EpubCaptureWithVault>> {
        Ok(None)
    }
}

impl KnowledgeCommand for CountingSplitStore {
    fn commit_text_capture(
        &mut self,
        _op: &str,
        _vault: &str,
        _draft: &CaptureDraft,
        _origin: &Provenance,
        _user_note: &str,
    ) -> Result<Receipt> {
        Err(KnowledgeError::new(
            KnowledgeErrorCode::Unavailable,
            "not used in scale test",
        ))
    }

    fn import_pdf_document(
        &mut self,
        op: &str,
        vault: &str,
        title: &str,
        original: &[u8],
        snapshot: &PdfSnapshot,
    ) -> Result<PdfDocumentReceipt> {
        if let Some(existing) = self.pdf_doc_ops.get(op) {
            return Ok(existing.clone());
        }
        let extraction_revision = snapshot
            .extraction_revision()
            .map_err(|e| KnowledgeError::new(KnowledgeErrorCode::InvalidInput, e.to_string()))?;
        self.next_rev += 1;
        self.next_doc_id += 1;
        let doc_id = self.next_doc_id;
        let receipt = PdfDocumentReceipt {
            operation_id: op.into(),
            document_id: doc_id,
            commit_rev: self.next_rev,
            original_sha256: snapshot.original_sha256.clone(),
            extraction_revision,
        };
        let record = PdfDocumentRecord {
            receipt: receipt.clone(),
            vault: vault.into(),
            title: title.into(),
            snapshot: snapshot.clone(),
        };
        self.pdf_blobs.insert(doc_id, original.to_vec());
        self.pdf_docs.insert(doc_id, record);
        self.pdf_doc_ops.insert(op.into(), receipt.clone());
        self.commits.insert(
            op.into(),
            CommitEntry {
                commit_rev: self.next_rev,
                operation_id: op.into(),
                kind: "pdf_import".into(),
                entity_id: doc_id,
                input_hash: sha256(op.as_bytes()),
                receipt: serde_json::to_string(&receipt).unwrap(),
            },
        );
        Ok(receipt)
    }

    fn capture_pdf(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        if let Some(cap_id) = self.pdf_cap_ops.get(op) {
            return Ok(self.pdf_captures[cap_id].receipt.clone());
        }
        let doc = self.pdf_docs.get(&document_id).ok_or_else(|| {
            KnowledgeError::new(KnowledgeErrorCode::NotFound, "PDF document not found")
        })?;
        self.next_rev += 1;
        self.next_cap_id += 1;
        let cap_id = self.next_cap_id;
        let receipt = PdfReceipt {
            operation_id: op.into(),
            capture_id: cap_id,
            document_id,
            commit_rev: self.next_rev,
            original_sha256: doc.receipt.original_sha256.clone(),
            extraction_revision: doc.receipt.extraction_revision.clone(),
            draft_hash: sha256(op.as_bytes()),
        };
        let record = PdfCaptureRecord {
            receipt: receipt.clone(),
            draft: draft.clone(),
            compound: None,
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        self.pdf_captures.insert(cap_id, record);
        self.pdf_cap_ops.insert(op.into(), cap_id);
        self.commits.insert(
            op.into(),
            CommitEntry {
                commit_rev: self.next_rev,
                operation_id: op.into(),
                kind: "pdf_capture".into(),
                entity_id: cap_id,
                input_hash: sha256(op.as_bytes()),
                receipt: serde_json::to_string(&receipt).unwrap(),
            },
        );
        Ok(receipt)
    }

    fn capture_pdf_compound(
        &mut self,
        _op: &str,
        _document_id: i64,
        _draft: &PdfCompoundDraft,
        _origin: &Provenance,
        _user_note: &str,
    ) -> Result<PdfReceipt> {
        Err(KnowledgeError::new(
            KnowledgeErrorCode::Unavailable,
            "not used in scale test",
        ))
    }

    fn import_epub(
        &mut self,
        _op: &str,
        _vault: &str,
        _snapshot: &EpubSnapshot,
    ) -> Result<EpubDocumentReceipt> {
        Err(KnowledgeError::new(
            KnowledgeErrorCode::Unavailable,
            "not used",
        ))
    }

    fn capture_epub_selection(
        &mut self,
        _op: &str,
        _vault: &str,
        _document_id: i64,
        _chapter_index: u32,
        _start: u64,
        _end: u64,
        _origin: &Provenance,
        _user_note: &str,
    ) -> Result<EpubReceipt> {
        Err(KnowledgeError::new(
            KnowledgeErrorCode::Unavailable,
            "not used",
        ))
    }

    fn update_pdf_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()> {
        let cap = self.pdf_captures.get_mut(&capture_id).ok_or_else(|| {
            KnowledgeError::new(KnowledgeErrorCode::NotFound, "capture not found")
        })?;
        let doc = self
            .pdf_docs
            .get(&cap.receipt.document_id)
            .ok_or_else(|| KnowledgeError::new(KnowledgeErrorCode::NotFound, "doc not found"))?;
        if doc.vault != vault {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "wrong vault",
            ));
        }
        cap.user_note = note.into();
        Ok(())
    }

    fn delete_pdf_capture(&mut self, _vault: &str, capture_id: i64) -> Result<()> {
        self.pdf_captures.remove(&capture_id);
        Ok(())
    }

    fn delete_pdf_document(&mut self, _vault: &str, document_id: i64) -> Result<()> {
        self.pdf_docs.remove(&document_id);
        Ok(())
    }

    fn update_epub_capture_note(
        &mut self,
        _vault: &str,
        _capture_id: i64,
        _note: &str,
    ) -> Result<()> {
        Ok(())
    }

    fn delete_epub_capture(&mut self, _vault: &str, _capture_id: i64) -> Result<()> {
        Ok(())
    }

    fn delete_epub_document(&mut self, _vault: &str, _document_id: i64) -> Result<()> {
        Ok(())
    }
}

impl KnowledgeArchive for CountingSplitStore {
    fn is_empty(&self) -> Result<bool> {
        Ok(self.commits.is_empty())
    }

    fn snapshot(&self) -> Result<ArchiveV2> {
        self.counters.snapshot_calls.fetch_add(1, Ordering::Relaxed);
        Err(KnowledgeError::new(
            KnowledgeErrorCode::Integrity,
            "normal reads and writes must not invoke KnowledgeArchive::snapshot",
        ))
    }

    fn restore(&mut self, _archive: &ArchiveV2) -> Result<()> {
        Err(KnowledgeError::new(
            KnowledgeErrorCode::Unavailable,
            "restore not used",
        ))
    }
}

impl StudyQuery for CountingSplitStore {
    fn study_decks(&self, vault: &str, now_ms: i64) -> Result<Vec<StudyDeckCounts>> {
        let mut out = Vec::new();
        for book in self.wordbooks.values().filter(|b| b.vault == vault) {
            let mut total = 0u32;
            let mut due = 0u32;
            let mut new_count = 0u32;
            for card in self.cards.values().filter(|c| c.wordbook_id == book.id) {
                total += 1;
                match self.schedules.get(&card.id) {
                    Some(s) if s.due_at_ms <= now_ms => due += 1,
                    Some(_) => {}
                    None => new_count += 1,
                }
            }
            out.push(StudyDeckCounts {
                wordbook: book.clone(),
                total,
                due,
                new_count,
            });
        }
        Ok(out)
    }

    fn wordbook_cards(
        &self,
        vault: &str,
        wordbook_id: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<StudyCard>> {
        let Some(book) = self
            .wordbooks
            .get(&wordbook_id)
            .filter(|b| b.vault == vault)
        else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "wordbook not found",
            ));
        };
        let mut cards: Vec<_> = self
            .cards
            .values()
            .filter(|c| c.wordbook_id == book.id)
            .cloned()
            .collect();
        cards.sort_by_key(|c| c.position);
        Ok(cards.into_iter().skip(offset).take(limit).collect())
    }

    fn study_queue(
        &self,
        vault: &str,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        limit: usize,
        daily_new_limit: u32,
    ) -> Result<StudyQueueData> {
        let _ = self
            .wordbooks
            .get(&wordbook_id)
            .filter(|b| b.vault == vault)
            .ok_or_else(|| KnowledgeError::new(KnowledgeErrorCode::NotFound, "wordbook missing"))?;
        let introduced_today = self
            .reviews
            .values()
            .filter(|r| r.reviewed_at_ms >= day_start_ms && r.stage_before == StudyStage::New)
            .count() as u32;
        let rem_new_budget = daily_new_limit.saturating_sub(introduced_today);
        let mut cards = Vec::new();
        let mut total_due = 0u32;
        let mut total_new = 0u32;
        for card in self.cards.values().filter(|c| c.wordbook_id == wordbook_id) {
            match self.schedules.get(&card.id) {
                Some(s) if s.due_at_ms <= now_ms => {
                    total_due += 1;
                    if cards.len() < limit {
                        cards.push(StudyQueueEntry {
                            card: card.clone(),
                            stage: s.stage,
                        });
                    }
                }
                Some(_) => {}
                None => {
                    total_new += 1;
                    if cards.len() < limit && (cards.len() as u32) < rem_new_budget {
                        cards.push(StudyQueueEntry {
                            card: card.clone(),
                            stage: StudyStage::New,
                        });
                    }
                }
            }
        }
        let picked_due = cards.iter().filter(|e| e.stage != StudyStage::New).count() as u32;
        let picked_new = cards.iter().filter(|e| e.stage == StudyStage::New).count() as u32;
        Ok(StudyQueueData {
            cards,
            remaining_due: total_due.saturating_sub(picked_due),
            remaining_new: total_new.min(rem_new_budget).saturating_sub(picked_new),
        })
    }

    fn card_review_context(
        &self,
        vault: &str,
        card_id: i64,
        operation_id: &str,
    ) -> Result<StudyReviewContext> {
        let card = self
            .cards
            .get(&card_id)
            .cloned()
            .ok_or_else(|| KnowledgeError::new(KnowledgeErrorCode::NotFound, "card missing"))?;
        let book = self
            .wordbooks
            .get(&card.wordbook_id)
            .ok_or_else(|| KnowledgeError::new(KnowledgeErrorCode::NotFound, "wordbook missing"))?;
        if book.vault != vault {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "wrong vault",
            ));
        }
        let existing_review = self
            .review_ops
            .get(operation_id)
            .and_then(|id| self.reviews.get(id))
            .cloned();
        Ok(StudyReviewContext {
            card: card.clone(),
            wordbook_id: book.id,
            existing_review,
            current_schedule: self.schedules.get(&card.id).cloned(),
        })
    }

    fn study_remaining_counts(
        &self,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        daily_new_limit: u32,
    ) -> Result<(u32, u32)> {
        let introduced_today = self
            .reviews
            .values()
            .filter(|r| r.reviewed_at_ms >= day_start_ms && r.stage_before == StudyStage::New)
            .count() as u32;
        let rem_new_budget = daily_new_limit.saturating_sub(introduced_today);
        let due = self
            .cards
            .values()
            .filter(|c| c.wordbook_id == wordbook_id)
            .filter(|c| matches!(self.schedules.get(&c.id), Some(s) if s.due_at_ms <= now_ms))
            .count() as u32;
        let unseen = self
            .cards
            .values()
            .filter(|c| c.wordbook_id == wordbook_id && !self.schedules.contains_key(&c.id))
            .count() as u32;
        Ok((due, unseen.min(rem_new_budget)))
    }
}

impl StudyCommand for CountingSplitStore {
    fn ensure_wordbook(
        &mut self,
        vault: &str,
        seed: &WordbookSeed,
        now_ms: i64,
    ) -> Result<Wordbook> {
        if let Some(existing) = self
            .wordbooks
            .values()
            .find(|b| b.vault == vault && b.title == seed.title)
        {
            return Ok(existing.clone());
        }
        self.next_book_id += 1;
        let book_id = self.next_book_id;
        let book = Wordbook {
            id: book_id,
            vault: vault.into(),
            title: seed.title.clone(),
            description: seed.description.clone(),
            created_at_ms: now_ms,
        };
        self.wordbooks.insert(book_id, book.clone());
        for (pos, c) in seed.words.iter().enumerate() {
            self.next_card_id += 1;
            let cid = self.next_card_id;
            self.cards.insert(
                cid,
                StudyCard {
                    id: cid,
                    wordbook_id: book_id,
                    position: pos as u32,
                    term: c.term.clone(),
                    pos: c.pos.clone(),
                    phonetic: c.phonetic.clone(),
                    meaning: c.meaning.clone(),
                    example: c.example.clone(),
                },
            );
        }
        Ok(book)
    }

    fn commit_review(
        &mut self,
        _guard: &ReviewGuard,
        schedule: &CardSchedule,
        review: &ReviewEntry,
    ) -> Result<ReviewEntry> {
        if let Some(id) = self.review_ops.get(&review.operation_id) {
            return Ok(self.reviews[id].clone());
        }
        self.next_review_id += 1;
        let rid = self.next_review_id;
        let mut stored = review.clone();
        stored.id = rid;
        self.schedules.insert(schedule.card_id, schedule.clone());
        self.reviews.insert(rid, stored.clone());
        self.review_ops.insert(review.operation_id.clone(), rid);
        Ok(stored)
    }

    fn delete_wordbook(&mut self, _vault: &str, wordbook_id: i64) -> Result<()> {
        self.wordbooks.remove(&wordbook_id);
        Ok(())
    }
}

impl StudyArchivePort for CountingSplitStore {
    fn study_snapshot(&self) -> Result<StudyArchive> {
        self.counters
            .study_snapshot_calls
            .fetch_add(1, Ordering::Relaxed);
        Err(KnowledgeError::new(
            KnowledgeErrorCode::Integrity,
            "normal study calls must not invoke StudyArchivePort::study_snapshot",
        ))
    }

    fn restore_study(&mut self, _archive: &StudyArchive) -> Result<()> {
        Err(KnowledgeError::new(
            KnowledgeErrorCode::Unavailable,
            "not used",
        ))
    }
}

fn large_pdf_fixture() -> (Vec<u8>, PdfSnapshot, PdfDraft) {
    let mut original = vec![b' '; 2 * 1024 * 1024];
    original[..9].copy_from_slice(b"%PDF-1.7\n");
    let page_text = "Alphabetagammadelta";
    let chars: Vec<_> = page_text
        .chars()
        .enumerate()
        .map(|(i, c)| {
            serde_json::json!({
                "pdf_char_index": i,
                "unicode_value": c as u32,
                "mapping": "scalar",
                "generated": false,
                "scalar_start": i,
                "scalar_end": i + 1,
                "utf8_start": i,
                "utf8_end": i + 1,
                "utf16_start": i,
                "utf16_end": i + 1,
                "pdf_bounds": { "left": i as f32, "bottom": 1.0, "right": i as f32 + 1.0, "top": 2.0 }
            })
        })
        .collect();
    let snapshot: PdfSnapshot = serde_json::from_value(serde_json::json!({
        "protocol_version": 1,
        "original_sha256": sha256(&original),
        "original_bytes": original.len(),
        "extraction": {
            "provider": "pdfium",
            "algorithm": "qusor-pdfium-char-map-v1",
            "wrapper": "pdfium-render-0.9.5",
            "abi": "pdfium_8125",
            "library_sha256": "a".repeat(64),
            "mapping_check": "unicode_scalar_validity_only",
            "normalization": PDF_NORMALIZATION
        },
        "engine_report_sha256": "b".repeat(64),
        "pages": [{
            "page_index": 0,
            "engine_text": page_text,
            "text_sha256": sha256(page_text.as_bytes()),
            "rotation_degrees": 0,
            "media_box": { "left": 0.0, "bottom": 0.0, "right": 100.0, "top": 100.0 },
            "crop_box": { "left": 0.0, "bottom": 0.0, "right": 100.0, "top": 100.0 },
            "characters": chars
        }]
    }))
    .unwrap();
    let draft = PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(page_text.as_bytes()),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: 9,
        exact: "Alphabeta".into(),
    };
    (original, snapshot, draft)
}

#[test]
fn large_vault_normal_reads_and_writes_never_touch_snapshot_or_original_blobs() {
    let counters = Counters::new();
    let store = CountingSplitStore::new(counters.clone());
    let mut app = KnowledgeApplication::new(store);

    let (pdf_bytes, snapshot, draft) = large_pdf_fixture();
    let doc_receipt = app
        .import_document("op-doc-1", "main", "Big PDF", &pdf_bytes, &snapshot)
        .unwrap();
    let origin = Provenance {
        package_id: "third.party".into(),
        publisher_id: "pub".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    };
    let frozen = FrozenPdfSource::freeze(
        &app,
        doc_receipt.document_id,
        "main",
        draft.clone(),
        origin.clone(),
    )
    .unwrap();
    assert_eq!(frozen.document_id(), doc_receipt.document_id);

    const CAPTURE_COUNT: usize = 2_000;
    let mut first_cap_id = 0;
    let mut last_cap_id = 0;
    for i in 0..CAPTURE_COUNT {
        let receipt = app
            .capture_pdf(
                &format!("op-cap-{i}"),
                doc_receipt.document_id,
                &draft,
                &origin,
                &format!("note body #{i}"),
            )
            .unwrap();
        if i == 0 {
            first_cap_id = receipt.capture_id;
        }
        last_cap_id = receipt.capture_id;
    }

    assert_eq!(counters.snapshot_calls.load(Ordering::Relaxed), 0);
    assert_eq!(counters.blob_reads.load(Ordering::Relaxed), 0);

    let docs_page = app.pdf_documents_page("main", None, 20).unwrap();
    assert_eq!(docs_page.documents.len(), 1);
    let mut seen_notes = 0usize;
    let mut cursor = None;
    loop {
        let page = app
            .pdf_notes_page(
                "main",
                Some(doc_receipt.document_id),
                cursor.as_deref(),
                100,
            )
            .unwrap();
        seen_notes += page.notes.len();
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen_notes, CAPTURE_COUNT);

    let quote = app
        .pdf_note_body("main", first_cap_id, NoteBodyField::Quote, 0, 512)
        .unwrap();
    assert_eq!(quote.text, "Alphabeta");
    let note_body = app
        .pdf_note_body("main", last_cap_id, NoteBodyField::Note, 0, 512)
        .unwrap();
    assert_eq!(note_body.text, format!("note body #{}", CAPTURE_COUNT - 1));

    app.update_pdf_capture_note("main", first_cap_id, "edited scale note")
        .unwrap();
    assert_eq!(
        app.pdf_page_text("main", doc_receipt.document_id, 0, 0, 1024)
            .unwrap()
            .text,
        "Alphabetagammadelta"
    );
    assert_eq!(
        app.pdf_saved_selections("main", doc_receipt.document_id, 0)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        app.query_document(doc_receipt.document_id).unwrap().title,
        "Big PDF"
    );
    assert_eq!(
        app.query_pdf_complete(first_cap_id).unwrap().user_note,
        "edited scale note"
    );
    assert_eq!(
        app.resolve_pdf(doc_receipt.document_id, &draft)
            .unwrap()
            .exact,
        "Alphabeta"
    );
    assert!(app.pdf_receipt("op-cap-0").unwrap().is_some());
    assert!(app.document_receipt("op-doc-1").unwrap().is_some());
    assert!(app.query_operation("op-cap-0").unwrap().is_some());

    let store = app.into_store();
    let mut study = StudyApplication::new(store);
    let seed = WordbookSeed {
        title: "Scale 2k".into(),
        description: "2000-card wordbook".into(),
        words: (0..2_000)
            .map(|i| NewWord {
                term: format!("term-{i}"),
                pos: "n.".into(),
                phonetic: "/t/".into(),
                meaning: format!("meaning-{i}"),
                example: format!("Example {i}."),
            })
            .collect(),
    };
    let now = 1_767_225_600_000i64;
    let book = study.seed_wordbook("main", &seed, now).unwrap();
    let decks = study.decks("main", now).unwrap();
    assert_eq!(decks[0].total, 2_000);
    let cards_page = study.wordbook_cards("main", book.id, 50, 100).unwrap();
    assert_eq!(cards_page.len(), 50);
    let queue = study.queue("main", book.id, now, now - 1_000, 10).unwrap();
    assert!(!queue.cards.is_empty());
    study
        .review(
            "main",
            queue.cards[0].card.id,
            StudyRating::Good,
            "op-study-rev-1",
            now,
            now - 1_000,
        )
        .unwrap();

    assert_eq!(counters.snapshot_calls.load(Ordering::Relaxed), 0);
    assert_eq!(counters.study_snapshot_calls.load(Ordering::Relaxed), 0);
    assert_eq!(counters.blob_reads.load(Ordering::Relaxed), 0);

    let app = KnowledgeApplication::new(study.into_store());
    let original = app.original_pdf(doc_receipt.document_id).unwrap();
    assert_eq!(original.len(), pdf_bytes.len());
    assert_eq!(counters.blob_reads.load(Ordering::Relaxed), 1);
    assert_eq!(counters.snapshot_calls.load(Ordering::Relaxed), 0);
}

struct QueryOnlyDouble;

impl KnowledgeQuery for QueryOnlyDouble {
    fn text_capture(&self, _id: i64) -> Result<Option<CaptureRecord>> {
        Ok(None)
    }
    fn text_receipt_by_operation(&self, _op: &str) -> Result<Option<Receipt>> {
        Ok(None)
    }
    fn commit_by_operation(&self, _op: &str) -> Result<Option<CommitEntry>> {
        Ok(None)
    }
    fn pdf_document(&self, _id: i64) -> Result<Option<PdfDocumentRecord>> {
        Ok(None)
    }
    fn pdf_document_receipt_by_operation(&self, _op: &str) -> Result<Option<PdfDocumentReceipt>> {
        Ok(None)
    }
    fn pdf_capture(&self, _id: i64) -> Result<Option<PdfCaptureRecord>> {
        Ok(None)
    }
    fn pdf_capture_with_vault(&self, _id: i64) -> Result<Option<PdfCaptureWithVault>> {
        Ok(None)
    }
    fn pdf_capture_by_operation(&self, _op: &str) -> Result<OperationLookup<PdfCaptureWithVault>> {
        Ok(OperationLookup::Absent)
    }
    fn pdf_documents_page(
        &self,
        _vault: &str,
        _after: Option<(i64, i64)>,
        _limit: usize,
    ) -> Result<PdfDocumentsPageData> {
        Ok(PdfDocumentsPageData {
            documents: vec![],
            next_cursor_key: None,
        })
    }
    fn pdf_page_text(
        &self,
        _vault: &str,
        _document_id: i64,
        _page_index: u32,
    ) -> Result<PdfPageTextLookup> {
        Ok(PdfPageTextLookup::DocumentMissing)
    }
    fn pdf_saved_selections(
        &self,
        _vault: &str,
        _document_id: i64,
        _page_index: u32,
    ) -> Result<PdfSavedSelectionsLookup> {
        Ok(PdfSavedSelectionsLookup::DocumentMissing)
    }
    fn pdf_notes_page(
        &self,
        _vault: &str,
        _document_id: Option<i64>,
        _after: Option<(i64, i64)>,
        _limit: usize,
    ) -> Result<PdfNotesPageData> {
        Ok(PdfNotesPageData {
            notes: vec![],
            next_cursor_key: None,
        })
    }
    fn read_original_pdf(&self, _id: i64) -> Result<Vec<u8>> {
        Err(KnowledgeError::new(KnowledgeErrorCode::NotFound, "missing"))
    }
    fn epub_documents_page(
        &self,
        _vault: &str,
        _after: Option<(i64, i64)>,
        _limit: usize,
    ) -> Result<EpubDocumentsPageData> {
        Ok(EpubDocumentsPageData {
            documents: vec![],
            next_cursor_key: None,
        })
    }
    fn epub_chapter_text(
        &self,
        _vault: &str,
        _document_id: i64,
        _chapter_index: u32,
    ) -> Result<EpubChapterLookup> {
        Ok(EpubChapterLookup::DocumentMissing)
    }
    fn epub_toc(&self, _vault: &str, _document_id: i64) -> Result<Option<Vec<EpubTocRecord>>> {
        Ok(None)
    }
    fn epub_saved_selections(
        &self,
        _vault: &str,
        _document_id: i64,
        _chapter_index: u32,
    ) -> Result<EpubSavedSelectionsLookup> {
        Ok(EpubSavedSelectionsLookup::DocumentMissing)
    }
    fn epub_notes_page(
        &self,
        _vault: &str,
        _document_id: Option<i64>,
        _after: Option<(i64, i64)>,
        _limit: usize,
    ) -> Result<EpubNotesPageData> {
        Ok(EpubNotesPageData {
            notes: vec![],
            next_cursor_key: None,
        })
    }
    fn epub_capture_with_vault(&self, _id: i64) -> Result<Option<EpubCaptureWithVault>> {
        Ok(None)
    }
}

impl StudyQuery for QueryOnlyDouble {
    fn study_decks(&self, _vault: &str, _now_ms: i64) -> Result<Vec<StudyDeckCounts>> {
        Ok(vec![])
    }
    fn wordbook_cards(
        &self,
        _vault: &str,
        _wordbook_id: i64,
        _limit: usize,
        _offset: usize,
    ) -> Result<Vec<StudyCard>> {
        Ok(vec![])
    }
    fn study_queue(
        &self,
        _vault: &str,
        _wordbook_id: i64,
        _now_ms: i64,
        _day_start_ms: i64,
        _limit: usize,
        _daily_new_limit: u32,
    ) -> Result<StudyQueueData> {
        Ok(StudyQueueData {
            cards: vec![],
            remaining_due: 0,
            remaining_new: 0,
        })
    }
    fn card_review_context(
        &self,
        _vault: &str,
        _card_id: i64,
        _operation_id: &str,
    ) -> Result<StudyReviewContext> {
        Err(KnowledgeError::new(KnowledgeErrorCode::NotFound, "missing"))
    }
    fn study_remaining_counts(
        &self,
        _wordbook_id: i64,
        _now_ms: i64,
        _day_start_ms: i64,
        _daily_new_limit: u32,
    ) -> Result<(u32, u32)> {
        Ok((0, 0))
    }
}

#[test]
fn query_only_adapter_compiles_and_serves_reads_without_command_or_archive() {
    let app = KnowledgeApplication::new(QueryOnlyDouble);
    assert!(
        app.pdf_documents_page("main", None, 10)
            .unwrap()
            .documents
            .is_empty()
    );
    let study = StudyApplication::new(QueryOnlyDouble);
    assert!(study.decks("main", 1_767_225_600_000).unwrap().is_empty());
}
