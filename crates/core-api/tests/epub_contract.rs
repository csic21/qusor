use core_api::{
    contract::sha256,
    epub_contract::{
        EPUB_PROTOCOL_VERSION, EPUB_TEXT_UNIT, EpubChapterSnapshot, EpubContractErrorCode,
        EpubDraft, EpubSnapshot, EpubTocEntry, MAX_EPUB_CHAPTER_BYTES, MAX_EPUB_CHAPTERS,
        MAX_EPUB_TOC_ENTRIES, resolve_epub,
    },
    knowledge::{ArchiveV2, KnowledgeErrorCode},
};

fn sample_snapshot() -> EpubSnapshot {
    let original = b"PK\x03\x04synthetic-epub-archive";
    EpubSnapshot {
        vault: "vault".into(),
        title: "Sample Book".into(),
        author: "Author Name".into(),
        original_sha256: sha256(original),
        original_bytes: original.len() as u64,
        extraction_revision: "qusor-epub-v1".into(),
        chapters: vec![
            EpubChapterSnapshot {
                chapter_index: 0,
                title: "Chapter 1".into(),
                href: "Text/ch1.xhtml".into(),
                text: "前\ne\u{301}👩‍💻后".into(),
            },
            EpubChapterSnapshot {
                chapter_index: 1,
                title: "Chapter 2".into(),
                href: "Text/ch2.xhtml".into(),
                text: "Second chapter body.".into(),
            },
        ],
        toc: vec![
            EpubTocEntry {
                title: "Chapter 1".into(),
                chapter_index: 0,
                depth: 0,
            },
            EpubTocEntry {
                title: "Chapter 2".into(),
                chapter_index: 1,
                depth: 1,
            },
        ],
    }
}

#[test]
fn snapshot_validates_bounds_and_computes_deterministic_hash() {
    let snap = sample_snapshot();
    assert!(snap.validate().is_ok());
    let h1 = snap.snapshot_hash().unwrap();
    let h2 = snap.snapshot_hash().unwrap();
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64);

    let mut empty_chapters = snap.clone();
    empty_chapters.chapters.clear();
    assert_eq!(
        empty_chapters.validate().unwrap_err().code,
        EpubContractErrorCode::BudgetExceeded
    );

    let mut too_many_chapters = snap.clone();
    too_many_chapters.chapters = (0..=MAX_EPUB_CHAPTERS)
        .map(|i| EpubChapterSnapshot {
            chapter_index: i as u32,
            title: "c".into(),
            href: format!("ch{i}.xhtml"),
            text: "ok".into(),
        })
        .collect();
    assert_eq!(
        too_many_chapters.validate().unwrap_err().code,
        EpubContractErrorCode::BudgetExceeded
    );

    let mut oversized_chapter = snap.clone();
    oversized_chapter.chapters[0].text = "a".repeat(MAX_EPUB_CHAPTER_BYTES + 1);
    assert_eq!(
        oversized_chapter.validate().unwrap_err().code,
        EpubContractErrorCode::BudgetExceeded
    );

    let mut too_many_toc = snap.clone();
    too_many_toc.toc = (0..=MAX_EPUB_TOC_ENTRIES)
        .map(|i| EpubTocEntry {
            title: format!("Entry {i}"),
            chapter_index: 0,
            depth: 0,
        })
        .collect();
    assert_eq!(
        too_many_toc.validate().unwrap_err().code,
        EpubContractErrorCode::BudgetExceeded
    );

    let mut bad_toc_target = snap.clone();
    bad_toc_target.toc[0].chapter_index = 99;
    assert_eq!(
        bad_toc_target.validate().unwrap_err().code,
        EpubContractErrorCode::Malformed
    );

    let mut non_contiguous = snap;
    non_contiguous.chapters[1].chapter_index = 2;
    assert_eq!(
        non_contiguous.validate().unwrap_err().code,
        EpubContractErrorCode::Malformed
    );
}

#[test]
fn resolves_exact_scalar_ranges_and_rejects_grapheme_splits() {
    let snap = sample_snapshot();
    // Chapter 0 text: "前\ne\u{301}👩‍💻后"
    // scalars:
    // 0: '前' (3 utf8, 1 utf16)
    // 1: '\n' (1 utf8, 1 utf16)
    // 2: 'e', 3: '\u{301}' (1+2 utf8, 1+1 utf16)
    // 4: '👩', 5: '\u{200d}', 6: '💻' (4+3+4 utf8, 2+1+2 utf16)
    // 7: '后' (3 utf8, 1 utf16)
    let draft = EpubDraft {
        protocol_version: EPUB_PROTOCOL_VERSION,
        original_sha256: snap.original_sha256.clone(),
        extraction_revision: snap.extraction_revision.clone(),
        chapter_index: 0,
        chapter_text_sha256: sha256(snap.chapters[0].text.as_bytes()),
        unit: EPUB_TEXT_UNIT.into(),
        start: 2,
        end: 7,
        exact: "e\u{301}👩‍💻".into(),
    };
    let sel = snap.resolve(&draft).unwrap();
    assert_eq!(sel.byte_start, 4);
    assert_eq!(sel.byte_end, 18);
    assert_eq!(sel.utf16_start, 2);
    assert_eq!(sel.utf16_end, 9);
    assert_eq!(sel.exact, "e\u{301}👩‍💻");

    // Splitting combining mark 'e' from '\u{301}' must fail
    let split_combining = EpubDraft {
        start: 2,
        end: 3,
        exact: "e".into(),
        ..draft.clone()
    };
    assert_eq!(
        resolve_epub(&snap, &split_combining).unwrap_err().code,
        EpubContractErrorCode::InvalidBoundary
    );

    // Splitting ZWJ emoji must fail
    let split_emoji = EpubDraft {
        start: 4,
        end: 5,
        exact: "👩".into(),
        ..draft.clone()
    };
    assert_eq!(
        resolve_epub(&snap, &split_emoji).unwrap_err().code,
        EpubContractErrorCode::InvalidBoundary
    );

    let wrong_quote = EpubDraft {
        exact: "wrong".into(),
        ..draft.clone()
    };
    assert_eq!(
        resolve_epub(&snap, &wrong_quote).unwrap_err().code,
        EpubContractErrorCode::QuoteMismatch
    );

    let wrong_hash = EpubDraft {
        chapter_text_sha256: "0".repeat(64),
        ..draft
    };
    assert_eq!(
        resolve_epub(&snap, &wrong_hash).unwrap_err().code,
        EpubContractErrorCode::HashMismatch
    );
}

#[test]
fn archive_v2_without_epub_fields_deserializes_and_validates() {
    let legacy_json = r#"{
        "schema_version": 2,
        "text_entries": [],
        "pdf_documents": [],
        "pdf_captures": [],
        "pdf_operations": [],
        "commits": [],
        "blobs": []
    }"#;
    let decoded: ArchiveV2 = serde_json::from_str(legacy_json).unwrap();
    assert!(decoded.epub_documents.is_empty());
    assert!(decoded.epub_captures.is_empty());
    assert!(decoded.validate_envelope().is_ok());

    let mut wrong_version = decoded;
    wrong_version.schema_version = 4;
    assert_eq!(
        wrong_version.validate_envelope().unwrap_err().code,
        KnowledgeErrorCode::UnsupportedVersion
    );
}
