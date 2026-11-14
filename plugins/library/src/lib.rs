#![cfg_attr(not(target_arch = "wasm32"), forbid(unsafe_code))]
#![deny(unsafe_code)]

pub mod plugin;

qusor_sdk::export_slot_plugin!(crate::plugin::LibraryPlugin);

#[cfg(test)]
mod tests {
    use crate::plugin::LibraryPlugin;
    use qusor_sdk::orthogonal::{
        CoreKnowledgeOperation, CoreKnowledgeRequest, CoreKnowledgeResponse, CoreKnowledgeResult,
        DocumentFormat, KnowledgeDocumentSummary, KnowledgePdfMetadata, KnowledgeSavedRange,
        KnowledgeTocEntry,
    };
    use qusor_sdk::ui::{
        ControlledSurfaceKind, PLUGIN_UI_PROTOCOL, UiAction, UiBudgets, UiComponent,
        UiEventEnvelope, UiNode,
    };
    use qusor_sdk::{
        HOST_API_V0_8, HostTransport, SdkError, SlotPlugin, SlotRenderContext, SlotUpdate,
    };
    use std::cell::RefCell;

    type TocEntryList = (DocumentFormat, i64, Vec<KnowledgeTocEntry>);
    type SavedRangeEntry = (DocumentFormat, i64, u32, Vec<KnowledgeSavedRange>);

    struct FakeHost {
        docs: RefCell<Vec<KnowledgeDocumentSummary>>,
        tocs: RefCell<Vec<TocEntryList>>,
        selections: RefCell<Vec<SavedRangeEntry>>,
        fail_next_delete: RefCell<bool>,
    }

    impl FakeHost {
        fn empty() -> Self {
            Self {
                docs: RefCell::new(Vec::new()),
                tocs: RefCell::new(Vec::new()),
                selections: RefCell::new(Vec::new()),
                fail_next_delete: RefCell::new(false),
            }
        }

        fn seeded() -> Self {
            Self {
                docs: RefCell::new(vec![
                    KnowledgeDocumentSummary {
                        document_id: 1,
                        format: DocumentFormat::Pdf,
                        commit_rev: 10,
                        title: "Rust Architecture PDF".into(),
                        section_count: 3,
                    },
                    KnowledgeDocumentSummary {
                        document_id: 2,
                        format: DocumentFormat::Epub,
                        commit_rev: 12,
                        title: "Orthogonal Plugins EPUB".into(),
                        section_count: 2,
                    },
                ]),
                tocs: RefCell::new(vec![(
                    DocumentFormat::Epub,
                    2,
                    vec![
                        KnowledgeTocEntry {
                            title: "Chapter 1: Capabilities".into(),
                            section_index: 0,
                            depth: 0,
                        },
                        KnowledgeTocEntry {
                            title: "Chapter 2: Slots".into(),
                            section_index: 1,
                            depth: 0,
                        },
                    ],
                )]),
                selections: RefCell::new(vec![
                    (
                        DocumentFormat::Epub,
                        2,
                        0,
                        vec![KnowledgeSavedRange {
                            capture_id: 71,
                            start: 0,
                            end: 18,
                        }],
                    ),
                    (
                        DocumentFormat::Pdf,
                        1,
                        0,
                        vec![KnowledgeSavedRange {
                            capture_id: 55,
                            start: 4,
                            end: 20,
                        }],
                    ),
                ]),
                fail_next_delete: RefCell::new(false),
            }
        }
    }

    impl HostTransport for FakeHost {
        fn call_bytes(&self, request: &[u8]) -> Result<Vec<u8>, SdkError> {
            let req = CoreKnowledgeRequest::from_slice(request)
                .map_err(|err| SdkError::Serialize(err.to_string()))?;
            let result = match req.operation {
                CoreKnowledgeOperation::DocumentsPage {
                    format,
                    cursor,
                    limit,
                } => {
                    let all: Vec<KnowledgeDocumentSummary> = self
                        .docs
                        .borrow()
                        .iter()
                        .filter(|d| format.is_none_or(|f| d.format == f))
                        .cloned()
                        .collect();
                    let offset = cursor
                        .as_deref()
                        .and_then(|c| c.strip_prefix("off:"))
                        .and_then(|n| n.parse::<usize>().ok())
                        .unwrap_or(0);
                    let end = offset.saturating_add(usize::from(limit)).min(all.len());
                    let next_cursor = if end < all.len() {
                        Some(format!("off:{end}"))
                    } else {
                        None
                    };
                    CoreKnowledgeResult::DocumentsPage {
                        documents: all[offset..end].to_vec(),
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
                        .map(|(_, _, e)| e.clone())
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
                        .map(|(_, _, _, r)| r.clone())
                        .unwrap_or_default();
                    CoreKnowledgeResult::SavedSelections {
                        format,
                        document_id,
                        section_index,
                        ranges,
                    }
                }
                CoreKnowledgeOperation::PdfMetadata { document_id } => {
                    CoreKnowledgeResult::PdfMetadata(KnowledgePdfMetadata {
                        document_id,
                        commit_rev: 10,
                        title: "Rust Architecture PDF".into(),
                        original_sha256: "a".repeat(64),
                        original_bytes: 2048,
                        extraction_revision: "b".repeat(64),
                        page_count: 3,
                    })
                }
                CoreKnowledgeOperation::DeleteDocument {
                    format,
                    document_id,
                } => {
                    if *self.fail_next_delete.borrow() {
                        *self.fail_next_delete.borrow_mut() = false;
                        return Err(SdkError::Transport("transient delete error".into()));
                    }
                    self.docs
                        .borrow_mut()
                        .retain(|d| !(d.format == format && d.document_id == document_id));
                    CoreKnowledgeResult::DocumentDeleted {
                        format,
                        document_id,
                        commit_rev: 20,
                    }
                }
                other => {
                    return Err(SdkError::Transport(format!(
                        "unexpected operation: {other:?}"
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

    fn press(node_id: &str, action_id: &str) -> UiEventEnvelope {
        UiEventEnvelope {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: "slot-1".into(),
            generation: 1,
            node_id: node_id.into(),
            action_id: action_id.into(),
            action: UiAction::Press,
        }
    }

    fn tab_change(tab_id: &str) -> UiEventEnvelope {
        UiEventEnvelope {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: "slot-1".into(),
            generation: 1,
            node_id: "library_format_tabs".into(),
            action_id: "filter_format".into(),
            action: UiAction::TabChanged {
                tab_id: tab_id.into(),
            },
        }
    }

    fn search_change(value: &str) -> UiEventEnvelope {
        UiEventEnvelope {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: "slot-1".into(),
            generation: 1,
            node_id: "library_search_input".into(),
            action_id: "search_changed".into(),
            action: UiAction::TextChanged {
                value: value.into(),
            },
        }
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
            _ => false,
        };
        hit || node.children.iter().any(|child| find_text(child, needle))
    }

    fn find_list_item(node: &UiNode, title_needle: &str) -> bool {
        let hit = matches!(
            &node.component,
            UiComponent::ListItem { title, .. } if title.contains(title_needle)
        );
        hit || node
            .children
            .iter()
            .any(|child| find_list_item(child, title_needle))
    }

    fn find_surface(
        node: &UiNode,
        expected_id: &str,
        expected_kind: ControlledSurfaceKind,
    ) -> bool {
        let hit = matches!(
            &node.component,
            UiComponent::ControlledSurfaceRef {
                surface_id,
                surface_kind,
            } if surface_id == expected_id && *surface_kind == expected_kind
        );
        hit || node
            .children
            .iter()
            .any(|child| find_surface(child, expected_id, expected_kind))
    }

    fn render_ctx(compact: bool) -> SlotRenderContext {
        SlotRenderContext {
            contribution_id: "org.qusor.library.workspace".into(),
            mount_id: "slot-1".into(),
            generation: 1,
            api_version: Some(HOST_API_V0_8),
            compact,
        }
    }

    #[test]
    fn empty_library_shows_honest_empty_state() {
        let host = FakeHost::empty();
        let mut plugin = LibraryPlugin::default();
        let tree = plugin.render(&render_ctx(false), &host).unwrap();
        tree.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&tree.root, "资料库为空"));
        assert!(find_text(&tree.root, "导入 PDF"));
        assert!(find_text(&tree.root, "导入 EPUB"));
    }

    #[test]
    fn library_filter_search_open_surface_section_delete_and_refresh() {
        let host = FakeHost::seeded();
        let mut plugin = LibraryPlugin::default();

        // 1. Initial list renders both PDF and EPUB.
        let initial = plugin.render(&render_ctx(false), &host).unwrap();
        initial.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&initial.root, "Rust Architecture PDF"));
        assert!(find_text(&initial.root, "Orthogonal Plugins EPUB"));

        // 2. Filter by EPUB tab hides PDF.
        let SlotUpdate::Tree(epub_only) = plugin.handle_event(&tab_change("epub"), &host).unwrap()
        else {
            panic!("expected tree");
        };
        epub_only.validate(UiBudgets::default()).unwrap();
        assert!(!find_text(&epub_only.root, "Rust Architecture PDF"));
        assert!(find_text(&epub_only.root, "Orthogonal Plugins EPUB"));

        // 3. Switch back to All, search for "Rust" -> filters locally.
        let _ = plugin.handle_event(&tab_change("all"), &host).unwrap();
        let SlotUpdate::Tree(searched) =
            plugin.handle_event(&search_change("Rust"), &host).unwrap()
        else {
            panic!("expected tree");
        };
        searched.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&searched.root, "Rust Architecture PDF"));
        assert!(!find_text(&searched.root, "Orthogonal Plugins EPUB"));

        // Clear search and trigger host import placeholder refresh.
        let _ = plugin.handle_event(&search_change(""), &host).unwrap();
        let SlotUpdate::Tree(refreshed) = plugin
            .handle_event(&press("import_pdf_btn", "host_import_pdf"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        refreshed.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&refreshed.root, "已请求宿主导入 PDF"));

        // 4. Open EPUB doc -> emits ControlledSurfaceRef("library-doc:epub:2:0") + TOC + saved selections.
        let SlotUpdate::Tree(detail) = plugin
            .handle_event(&press("doc_item_epub_2", "open_doc:epub:2"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        detail.validate(UiBudgets::default()).unwrap();
        assert!(find_surface(
            &detail.root,
            "library-doc:epub:2:0",
            ControlledSurfaceKind::ReaderDocument
        ));
        assert!(find_text(&detail.root, "Chapter 1: Capabilities"));
        assert!(find_text(&detail.root, "摘录 #71"));

        // 5. Switch to next section -> surface_id updates to section 1.
        let SlotUpdate::Tree(sec1) = plugin
            .handle_event(&press("next_section_btn", "next_section"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        sec1.validate(UiBudgets::default()).unwrap();
        assert!(find_surface(
            &sec1.root,
            "library-doc:epub:2:1",
            ControlledSurfaceKind::ReaderDocument
        ));

        // 6. Two-step delete: request_delete -> cancel_delete -> request_delete -> confirm_delete.
        let SlotUpdate::Tree(confirm_prompt) = plugin
            .handle_event(&press("detail_delete_btn", "request_delete"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        confirm_prompt.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&confirm_prompt.root, "确认删除"));
        assert_eq!(host.docs.borrow().len(), 2);

        let SlotUpdate::Tree(cancelled) = plugin
            .handle_event(&press("cancel_delete_btn", "cancel_delete"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        cancelled.validate(UiBudgets::default()).unwrap();
        assert!(!find_text(&cancelled.root, "危险操作 · 两步确认"));
        assert_eq!(host.docs.borrow().len(), 2);

        let _ = plugin
            .handle_event(&press("detail_delete_btn", "request_delete"), &host)
            .unwrap();
        let SlotUpdate::Tree(deleted) = plugin
            .handle_event(&press("confirm_delete_btn", "confirm_delete"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        deleted.validate(UiBudgets::default()).unwrap();
        assert_eq!(host.docs.borrow().len(), 1);
        assert!(!find_list_item(&deleted.root, "Orthogonal Plugins EPUB"));
        assert!(find_list_item(&deleted.root, "Rust Architecture PDF"));
        assert!(find_text(
            &deleted.root,
            "已删除《Orthogonal Plugins EPUB》"
        ));
    }
}
