#![cfg_attr(not(target_arch = "wasm32"), forbid(unsafe_code))]
#![deny(unsafe_code)]

pub mod plugin;

qusor_sdk::export_slot_plugin!(crate::plugin::ReaderPlugin);

#[cfg(test)]
mod tests {
    use crate::plugin::ReaderPlugin;
    use qusor_sdk::ui::{
        ControlledSurfaceKind, PLUGIN_UI_PROTOCOL, UiAction, UiBudgets, UiComponent,
        UiEventEnvelope, UiNode,
    };
    use qusor_sdk::{
        CoreKnowledgeOperation, CoreKnowledgeRequest, CoreKnowledgeResponse, CoreKnowledgeResult,
        DocumentFormat, HOST_API_V0_8, HostTransport, KnowledgeDocumentSummary,
        KnowledgeNoteSummary, KnowledgeSavedRange, KnowledgeTocEntry, SdkError, SlotNavigation,
        SlotPlugin, SlotRenderContext, SlotUpdate,
    };
    use std::cell::RefCell;

    type TocEntryRecord = (DocumentFormat, i64, Vec<KnowledgeTocEntry>);
    type SavedSelectionRecord = (DocumentFormat, i64, u32, Vec<KnowledgeSavedRange>);

    struct FakeHost {
        documents: RefCell<Vec<KnowledgeDocumentSummary>>,
        tocs: RefCell<Vec<TocEntryRecord>>,
        selections: RefCell<Vec<SavedSelectionRecord>>,
        notes: RefCell<Vec<KnowledgeNoteSummary>>,
        seen_ops: RefCell<Vec<CoreKnowledgeOperation>>,
        documents_page_size: usize,
        repeat_cursor: Option<String>,
    }

    impl FakeHost {
        fn empty() -> Self {
            Self {
                documents: RefCell::new(Vec::new()),
                tocs: RefCell::new(Vec::new()),
                selections: RefCell::new(Vec::new()),
                notes: RefCell::new(Vec::new()),
                seen_ops: RefCell::new(Vec::new()),
                documents_page_size: usize::MAX,
                repeat_cursor: None,
            }
        }

        fn seeded() -> Self {
            Self {
                documents: RefCell::new(vec![
                    KnowledgeDocumentSummary {
                        document_id: 10,
                        format: DocumentFormat::Pdf,
                        commit_rev: 2,
                        title: "Rust Wasm Runtime".into(),
                        section_count: 3,
                    },
                    KnowledgeDocumentSummary {
                        document_id: 20,
                        format: DocumentFormat::Epub,
                        commit_rev: 4,
                        title: "Local First Architecture".into(),
                        section_count: 2,
                    },
                ]),
                tocs: RefCell::new(vec![
                    (
                        DocumentFormat::Epub,
                        20,
                        vec![
                            KnowledgeTocEntry {
                                title: "Chapter 1: Foundations".into(),
                                section_index: 0,
                                depth: 0,
                            },
                            KnowledgeTocEntry {
                                title: "Chapter 2: Sandboxing".into(),
                                section_index: 1,
                                depth: 0,
                            },
                        ],
                    ),
                    (DocumentFormat::Pdf, 10, vec![]),
                ]),
                selections: RefCell::new(vec![
                    (
                        DocumentFormat::Epub,
                        20,
                        0,
                        vec![KnowledgeSavedRange {
                            capture_id: 101,
                            start: 0,
                            end: 18,
                        }],
                    ),
                    (
                        DocumentFormat::Epub,
                        20,
                        1,
                        vec![
                            KnowledgeSavedRange {
                                capture_id: 102,
                                start: 5,
                                end: 24,
                            },
                            KnowledgeSavedRange {
                                capture_id: 103,
                                start: 30,
                                end: 42,
                            },
                        ],
                    ),
                ]),
                notes: RefCell::new(vec![
                    KnowledgeNoteSummary {
                        format: DocumentFormat::Epub,
                        capture_id: 101,
                        commit_rev: 3,
                        document_id: 20,
                        document_title: "Local First Architecture".into(),
                        section_index: 0,
                        quote_preview: "Local state stays yours.".into(),
                        quote_bytes: 24,
                        quote_truncated: false,
                        note_preview: "Core principle".into(),
                        note_bytes: 14,
                        note_truncated: false,
                    },
                    KnowledgeNoteSummary {
                        format: DocumentFormat::Epub,
                        capture_id: 102,
                        commit_rev: 4,
                        document_id: 20,
                        document_title: "Local First Architecture".into(),
                        section_index: 1,
                        quote_preview: "Capabilities are explicitly granted.".into(),
                        quote_bytes: 36,
                        quote_truncated: false,
                        note_preview: "Sandbox rule".into(),
                        note_bytes: 12,
                        note_truncated: false,
                    },
                ]),
                seen_ops: RefCell::new(Vec::new()),
                documents_page_size: usize::MAX,
                repeat_cursor: None,
            }
        }

        fn with_documents_page_size(mut self, size: usize) -> Self {
            self.documents_page_size = size.max(1);
            self
        }

        fn repeated_cursor(mut self, cursor: &str) -> Self {
            self.repeat_cursor = Some(cursor.to_owned());
            self
        }

        fn operations(&self) -> Vec<CoreKnowledgeOperation> {
            self.seen_ops.borrow().clone()
        }

        fn clear_operations(&self) {
            self.seen_ops.borrow_mut().clear();
        }

        fn remove_capture(&self, capture_id: i64) {
            for (_, _, _, ranges) in self.selections.borrow_mut().iter_mut() {
                ranges.retain(|range| range.capture_id != capture_id);
            }
            self.remove_note(capture_id);
        }

        fn remove_note(&self, capture_id: i64) {
            self.notes
                .borrow_mut()
                .retain(|note| note.capture_id != capture_id);
        }

        fn move_note_section(&self, capture_id: i64, section_index: u32) {
            for note in self
                .notes
                .borrow_mut()
                .iter_mut()
                .filter(|note| note.capture_id == capture_id)
            {
                note.section_index = section_index;
            }
        }
    }

    impl HostTransport for FakeHost {
        fn call_bytes(&self, request: &[u8]) -> Result<Vec<u8>, SdkError> {
            let req = CoreKnowledgeRequest::from_slice(request)
                .map_err(|err| SdkError::Serialize(err.to_string()))?;
            self.seen_ops.borrow_mut().push(req.operation.clone());
            let result = match req.operation {
                CoreKnowledgeOperation::DocumentsPage { format, cursor, .. } => {
                    let offset = cursor
                        .as_deref()
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(0);
                    let matching: Vec<KnowledgeDocumentSummary> = self
                        .documents
                        .borrow()
                        .iter()
                        .filter(|doc| format.is_none_or(|f| doc.format == f))
                        .cloned()
                        .collect();
                    let end = offset
                        .saturating_add(self.documents_page_size)
                        .min(matching.len());
                    let documents = matching.get(offset..end).unwrap_or(&[]).to_vec();
                    let next_cursor = if end < matching.len() {
                        Some(end.to_string())
                    } else {
                        self.repeat_cursor.clone()
                    };
                    CoreKnowledgeResult::DocumentsPage {
                        documents,
                        next_cursor,
                    }
                }
                CoreKnowledgeOperation::DocumentToc {
                    format,
                    document_id,
                } => {
                    let entries = self
                        .tocs
                        .borrow()
                        .iter()
                        .find(|(f, id, _)| *f == format && *id == document_id)
                        .map(|(_, _, entries)| entries.clone())
                        .unwrap_or_default();
                    CoreKnowledgeResult::DocumentToc {
                        format,
                        document_id,
                        entries,
                    }
                }
                CoreKnowledgeOperation::SavedSelections {
                    format,
                    document_id,
                    section_index,
                } => {
                    let ranges = self
                        .selections
                        .borrow()
                        .iter()
                        .find(|(f, id, sec, _)| {
                            *f == format && *id == document_id && *sec == section_index
                        })
                        .map(|(_, _, _, ranges)| ranges.clone())
                        .unwrap_or_default();
                    CoreKnowledgeResult::SavedSelections {
                        format,
                        document_id,
                        section_index,
                        ranges,
                    }
                }
                CoreKnowledgeOperation::NotesPage {
                    format,
                    document_id,
                    ..
                } => {
                    let notes = self
                        .notes
                        .borrow()
                        .iter()
                        .filter(|n| {
                            format.is_none_or(|f| n.format == f)
                                && document_id.is_none_or(|id| n.document_id == id)
                        })
                        .cloned()
                        .collect();
                    CoreKnowledgeResult::NotesPage {
                        notes,
                        next_cursor: None,
                    }
                }
                other => {
                    return Err(SdkError::Transport(format!(
                        "unexpected operation in reader unit test: {other:?}"
                    )));
                }
            };
            let resp = CoreKnowledgeResponse {
                request_id: req.request_id,
                result,
            };
            Ok(serde_json::to_vec(&resp).unwrap())
        }
    }

    fn event(node_id: &str, action_id: &str, action: UiAction) -> UiEventEnvelope {
        UiEventEnvelope {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: "slot-1".into(),
            generation: 1,
            node_id: node_id.into(),
            action_id: action_id.into(),
            action,
        }
    }

    fn render_ctx() -> SlotRenderContext {
        SlotRenderContext {
            contribution_id: "org.qusor.reader.preview.workspace".into(),
            mount_id: "slot-1".into(),
            generation: 1,
            api_version: Some(HOST_API_V0_8),
            compact: false,
            navigation: None,
        }
    }

    fn navigation_ctx(token: &str, payload: serde_json::Value) -> SlotRenderContext {
        SlotRenderContext {
            navigation: Some(SlotNavigation {
                route: "source_capture".into(),
                launch_token: token.into(),
                payload,
            }),
            ..render_ctx()
        }
    }

    fn valid_navigation() -> serde_json::Value {
        serde_json::json!({
            "format": "epub",
            "document_id": 20,
            "section_index": 1,
            "capture_id": 102,
        })
    }

    fn find_text(node: &UiNode, needle: &str) -> bool {
        let hit = match &node.component {
            UiComponent::Text { content } => content.contains(needle),
            UiComponent::ListItem {
                title, subtitle, ..
            } => title.contains(needle) || subtitle.as_deref().is_some_and(|s| s.contains(needle)),
            UiComponent::Button { label, .. } | UiComponent::Badge { label, .. } => {
                label.contains(needle)
            }
            UiComponent::KeyValue { key, value } => key.contains(needle) || value.contains(needle),
            UiComponent::EmptyState { title, description } => {
                title.contains(needle) || description.contains(needle)
            }
            UiComponent::Section { title } => title.contains(needle),
            _ => false,
        };
        hit || node.children.iter().any(|child| find_text(child, needle))
    }

    fn find_surface(node: &UiNode) -> Option<(String, ControlledSurfaceKind)> {
        if let UiComponent::ControlledSurfaceRef {
            surface_id,
            surface_kind,
        } = &node.component
        {
            return Some((surface_id.clone(), *surface_kind));
        }
        node.children.iter().find_map(find_surface)
    }

    #[test]
    fn empty_vault_renders_honest_empty_state_without_fake_books() {
        let host = FakeHost::empty();
        let mut plugin = ReaderPlugin::default();
        let tree = plugin.render(&render_ctx(), &host).unwrap();
        tree.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&tree.root, "知识库暂无可阅读资料"));
        assert!(!find_text(&tree.root, "准原生应用的秩序"));
        assert!(find_surface(&tree.root).is_none());
    }

    #[test]
    fn shelf_filter_search_open_toc_jump_and_saved_selections_sync() {
        let host = FakeHost::seeded();
        let mut plugin = ReaderPlugin::default();

        // 1. Initial shelf loads both PDF and EPUB documents.
        let shelf = plugin.render(&render_ctx(), &host).unwrap();
        shelf.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&shelf.root, "Rust Wasm Runtime"));
        assert!(find_text(&shelf.root, "Local First Architecture"));

        // 2. Filter to EPUB tab only.
        let SlotUpdate::Tree(epub_shelf) = plugin
            .handle_event(
                &event(
                    "reader_format_tabs",
                    "filter_format",
                    UiAction::TabChanged {
                        tab_id: "epub".into(),
                    },
                ),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        epub_shelf.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&epub_shelf.root, "Local First Architecture"));
        assert!(!find_text(&epub_shelf.root, "Rust Wasm Runtime"));

        // 3. Search filters cleanly without forging data.
        let SlotUpdate::Tree(search_miss) = plugin
            .handle_event(
                &event(
                    "reader_search_input",
                    "search_changed",
                    UiAction::TextChanged {
                        value: "nonexistent".into(),
                    },
                ),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        assert!(find_text(&search_miss.root, "没有匹配的资料"));

        let SlotUpdate::Tree(search_cleared) = plugin
            .handle_event(
                &event("reader_search_clear_btn", "clear_search", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        assert!(find_text(&search_cleared.root, "Local First Architecture"));

        // 4. Open EPUB document -> renders TOC, ControlledSurfaceRef, and section 0 highlights.
        let SlotUpdate::Tree(opened) = plugin
            .handle_event(
                &event("doc_item_epub_20", "open_doc:epub:20", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        opened.validate(UiBudgets::default()).unwrap();
        assert_eq!(
            find_surface(&opened.root),
            Some((
                "reader-doc:epub:20:0".to_owned(),
                ControlledSurfaceKind::ReaderDocument
            ))
        );
        assert!(find_text(&opened.root, "Chapter 1: Foundations"));
        assert!(find_text(&opened.root, "Chapter 2: Sandboxing"));
        assert!(find_text(&opened.root, "已保存高亮 1 处"));
        assert!(find_text(&opened.root, "Local state stays yours."));

        // 5. Jump to chapter 2 via TOC -> updates surface_id and section 1 saved selections.
        let SlotUpdate::Tree(jumped) = plugin
            .handle_event(
                &event("reader_toc_item_1", "jump_section:1", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        jumped.validate(UiBudgets::default()).unwrap();
        assert_eq!(
            find_surface(&jumped.root),
            Some((
                "reader-doc:epub:20:1".to_owned(),
                ControlledSurfaceKind::ReaderDocument
            ))
        );
        assert!(find_text(&jumped.root, "已保存高亮 2 处"));
        assert!(find_text(
            &jumped.root,
            "Capabilities are explicitly granted."
        ));

        // 6. Prev section button returns to chapter 1 (section 0).
        let SlotUpdate::Tree(prev_tree) = plugin
            .handle_event(
                &event("reader_prev_btn", "prev_section", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        prev_tree.validate(UiBudgets::default()).unwrap();
        assert_eq!(
            find_surface(&prev_tree.root),
            Some((
                "reader-doc:epub:20:0".to_owned(),
                ControlledSurfaceKind::ReaderDocument
            ))
        );

        // 7. Return to shelf and switch to All -> open PDF document and advance page.
        let _ = plugin
            .handle_event(
                &event("reader_back_btn", "back_to_shelf", UiAction::Press),
                &host,
            )
            .unwrap();
        let _ = plugin
            .handle_event(
                &event(
                    "reader_format_tabs",
                    "filter_format",
                    UiAction::TabChanged {
                        tab_id: "all".into(),
                    },
                ),
                &host,
            )
            .unwrap();
        let SlotUpdate::Tree(pdf_opened) = plugin
            .handle_event(
                &event("doc_item_pdf_10", "open_doc:pdf:10", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        pdf_opened.validate(UiBudgets::default()).unwrap();
        assert_eq!(
            find_surface(&pdf_opened.root),
            Some((
                "reader-doc:pdf:10:0".to_owned(),
                ControlledSurfaceKind::ReaderDocument
            ))
        );
        let SlotUpdate::Tree(pdf_next) = plugin
            .handle_event(
                &event("reader_next_btn", "next_section", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        pdf_next.validate(UiBudgets::default()).unwrap();
        assert_eq!(
            find_surface(&pdf_next.root),
            Some((
                "reader-doc:pdf:10:1".to_owned(),
                ControlledSurfaceKind::ReaderDocument
            ))
        );
    }

    #[test]
    fn source_navigation_consumes_each_token_once_for_success_and_failure() {
        let host = FakeHost::seeded();
        let mut plugin = ReaderPlugin::default();

        let success_ctx = navigation_ctx("launch-success", valid_navigation());
        let first = plugin.render(&success_ctx, &host).unwrap();
        first.validate(UiBudgets::default()).unwrap();
        assert_eq!(
            find_surface(&first.root).map(|(surface, _)| surface),
            Some("reader-doc:focus-102:epub:20:1".to_owned())
        );
        assert!(!host.operations().is_empty());

        host.clear_operations();
        let repeated = plugin.render(&success_ctx, &host).unwrap();
        repeated.validate(UiBudgets::default()).unwrap();
        assert!(host.operations().is_empty());
        assert_eq!(
            find_surface(&repeated.root).map(|(surface, _)| surface),
            Some("reader-doc:focus-102:epub:20:1".to_owned())
        );

        let mut invalid = valid_navigation();
        invalid["capture_id"] = serde_json::json!(999);
        let failure_ctx = navigation_ctx("launch-failure", invalid);
        let first_failure = plugin.render(&failure_ctx, &host).unwrap();
        first_failure.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&first_failure.root, "摘录 #999"));
        assert_eq!(host.operations().len(), 3);

        host.clear_operations();
        let repeated_failure = plugin.render(&failure_ctx, &host).unwrap();
        repeated_failure.validate(UiBudgets::default()).unwrap();
        assert!(host.operations().is_empty());
        assert!(find_text(&repeated_failure.root, "摘录 #999"));
        assert_eq!(first_failure.root, repeated_failure.root);
    }

    #[test]
    fn source_navigation_is_not_replayed_after_ordinary_transitions() {
        let host = FakeHost::seeded();
        let mut plugin = ReaderPlugin::default();
        let navigation = navigation_ctx("launch-once", valid_navigation());

        let tree = plugin.render(&navigation, &host).unwrap();
        assert_eq!(
            find_surface(&tree.root).map(|(surface, _)| surface),
            Some("reader-doc:focus-102:epub:20:1".to_owned())
        );

        let SlotUpdate::Tree(switched) = plugin
            .handle_event(
                &event("reader_prev_btn", "prev_section", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        assert_eq!(
            find_surface(&switched.root).map(|(surface, _)| surface),
            Some("reader-doc:epub:20:0".to_owned())
        );

        let _ = plugin
            .handle_event(
                &event("reader_back_btn", "back_to_shelf", UiAction::Press),
                &host,
            )
            .unwrap();
        let _ = plugin
            .handle_event(
                &event("reader_refresh_btn", "refresh_library", UiAction::Press),
                &host,
            )
            .unwrap();
        let _ = plugin
            .handle_event(
                &event("doc_item_epub_20", "open_doc:epub:20", UiAction::Press),
                &host,
            )
            .unwrap();
        host.clear_operations();

        let repeated = plugin.render(&navigation, &host).unwrap();
        assert!(host.operations().is_empty());
        assert_eq!(
            find_surface(&repeated.root).map(|(surface, _)| surface),
            Some("reader-doc:epub:20:0".to_owned())
        );
    }

    #[test]
    fn source_navigation_finds_epub_with_unfiltered_page_and_keeps_shelf_filter() {
        let host = FakeHost::seeded();
        let mut plugin = ReaderPlugin::default();
        plugin.render(&render_ctx(), &host).unwrap();
        let _ = plugin
            .handle_event(
                &event(
                    "reader_format_tabs",
                    "filter_format",
                    UiAction::TabChanged {
                        tab_id: "pdf".into(),
                    },
                ),
                &host,
            )
            .unwrap();
        host.clear_operations();

        let tree = plugin
            .render(
                &navigation_ctx("launch-unfiltered", valid_navigation()),
                &host,
            )
            .unwrap();
        assert_eq!(
            find_surface(&tree.root).map(|(surface, _)| surface),
            Some("reader-doc:focus-102:epub:20:1".to_owned())
        );
        let page_formats: Vec<Option<DocumentFormat>> = host
            .operations()
            .into_iter()
            .filter_map(|operation| match operation {
                CoreKnowledgeOperation::DocumentsPage { format, .. } => Some(format),
                _ => None,
            })
            .collect();
        assert!(!page_formats.is_empty());
        assert!(page_formats.iter().all(Option::is_none));

        let _ = plugin
            .handle_event(
                &event("reader_back_btn", "back_to_shelf", UiAction::Press),
                &host,
            )
            .unwrap();
        let SlotUpdate::Tree(shelf) = plugin
            .handle_event(
                &event("reader_refresh_btn", "refresh_library", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        assert!(find_text(&shelf.root, "Rust Wasm Runtime"));
        assert!(!find_text(&shelf.root, "Local First Architecture"));
    }

    #[test]
    fn direct_open_document_uses_unfiltered_page_without_changing_shelf_filter() {
        let host = FakeHost::seeded();
        let mut plugin = ReaderPlugin::default();
        plugin.render(&render_ctx(), &host).unwrap();
        let _ = plugin
            .handle_event(
                &event(
                    "reader_format_tabs",
                    "filter_format",
                    UiAction::TabChanged {
                        tab_id: "pdf".into(),
                    },
                ),
                &host,
            )
            .unwrap();
        host.clear_operations();

        let SlotUpdate::Tree(opened) = plugin
            .handle_event(
                &event("doc_item_epub_20", "open_doc:epub:20", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        opened.validate(UiBudgets::default()).unwrap();
        assert_eq!(
            find_surface(&opened.root).map(|(surface, _)| surface),
            Some("reader-doc:epub:20:0".to_owned())
        );
        let page_formats: Vec<Option<DocumentFormat>> = host
            .operations()
            .into_iter()
            .filter_map(|operation| match operation {
                CoreKnowledgeOperation::DocumentsPage { format, .. } => Some(format),
                _ => None,
            })
            .collect();
        assert!(!page_formats.is_empty());
        assert!(page_formats.iter().all(Option::is_none));

        let _ = plugin
            .handle_event(
                &event("reader_back_btn", "back_to_shelf", UiAction::Press),
                &host,
            )
            .unwrap();
        let SlotUpdate::Tree(shelf) = plugin
            .handle_event(
                &event("reader_refresh_btn", "refresh_library", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        assert!(find_text(&shelf.root, "Rust Wasm Runtime"));
        assert!(!find_text(&shelf.root, "Local First Architecture"));
    }

    #[test]
    fn document_search_stops_on_unadvanced_and_repeated_cursors() {
        let no_progress = FakeHost::seeded().repeated_cursor("loop");
        let mut plugin = ReaderPlugin::default();
        let missing = serde_json::json!({
            "format": "epub",
            "document_id": 999,
            "section_index": 0,
            "capture_id": 1,
        });
        plugin
            .render(
                &navigation_ctx("launch-loop", missing.clone()),
                &no_progress,
            )
            .unwrap();
        let loop_pages = no_progress
            .operations()
            .into_iter()
            .filter(|operation| matches!(operation, CoreKnowledgeOperation::DocumentsPage { .. }))
            .count();
        assert_eq!(loop_pages, 2);

        let repeated = FakeHost::seeded()
            .with_documents_page_size(1)
            .repeated_cursor("0");
        let mut plugin = ReaderPlugin::default();
        plugin
            .render(&navigation_ctx("launch-repeat", missing), &repeated)
            .unwrap();
        let cursor_pages = repeated
            .operations()
            .into_iter()
            .filter_map(|operation| match operation {
                CoreKnowledgeOperation::DocumentsPage { cursor, .. } => Some(cursor),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            cursor_pages,
            vec![None, Some("1".to_owned()), Some("0".to_owned())]
        );
    }

    #[test]
    fn refresh_reader_invalidates_focus_when_capture_or_note_is_gone() {
        let host = FakeHost::seeded();
        let mut plugin = ReaderPlugin::default();
        let tree = plugin
            .render(&navigation_ctx("launch-focus", valid_navigation()), &host)
            .unwrap();
        assert_eq!(
            find_surface(&tree.root).map(|(surface, _)| surface),
            Some("reader-doc:focus-102:epub:20:1".to_owned())
        );

        host.remove_capture(102);
        let SlotUpdate::Tree(refreshed) = plugin
            .handle_event(
                &event("reader_view_refresh_btn", "refresh_reader", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        refreshed.validate(UiBudgets::default()).unwrap();
        assert_eq!(
            find_surface(&refreshed.root).map(|(surface, _)| surface),
            Some("reader-doc:epub:20:1".to_owned())
        );
        assert!(find_text(
            &refreshed.root,
            "摘录 #102 已不存在，已退出精确回源。"
        ));

        let host = FakeHost::seeded();
        let mut plugin = ReaderPlugin::default();
        plugin
            .render(
                &navigation_ctx("launch-focus-note", valid_navigation()),
                &host,
            )
            .unwrap();
        host.remove_note(102);
        let SlotUpdate::Tree(refreshed) = plugin
            .handle_event(
                &event("reader_view_refresh_btn", "refresh_reader", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        assert_eq!(
            find_surface(&refreshed.root).map(|(surface, _)| surface),
            Some("reader-doc:epub:20:1".to_owned())
        );
        assert!(find_text(
            &refreshed.root,
            "摘录 #102 已不存在，已退出精确回源。"
        ));

        let host = FakeHost::seeded();
        let mut plugin = ReaderPlugin::default();
        plugin
            .render(
                &navigation_ctx("launch-focus-section", valid_navigation()),
                &host,
            )
            .unwrap();
        host.move_note_section(102, 0);
        let SlotUpdate::Tree(refreshed) = plugin
            .handle_event(
                &event("reader_view_refresh_btn", "refresh_reader", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        assert_eq!(
            find_surface(&refreshed.root).map(|(surface, _)| surface),
            Some("reader-doc:epub:20:1".to_owned())
        );
        assert!(find_text(
            &refreshed.root,
            "摘录 #102 已不存在，已退出精确回源。"
        ));
    }
}
