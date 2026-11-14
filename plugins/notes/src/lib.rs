#![cfg_attr(not(target_arch = "wasm32"), forbid(unsafe_code))]
#![deny(unsafe_code)]

pub mod plugin;

qusor_sdk::export_slot_plugin!(crate::plugin::NotesPlugin);

#[cfg(test)]
mod tests {
    use crate::plugin::NotesPlugin;
    use qusor_sdk::orthogonal::{
        CoreKnowledgeOperation, CoreKnowledgeRequest, CoreKnowledgeResponse, CoreKnowledgeResult,
        DocumentFormat, KnowledgeNoteBody, KnowledgeNoteSummary, MAX_KNOWLEDGE_NOTE_BYTES,
    };
    use qusor_sdk::ui::{
        PLUGIN_UI_PROTOCOL, UiAction, UiBudgets, UiComponent, UiEventEnvelope, UiNode,
    };
    use qusor_sdk::{
        ApiError, ErrorCode, HOST_API_V0_8, HostTransport, NoteField, SdkError, SlotPlugin,
        SlotRenderContext, SlotUpdate,
    };
    use std::cell::RefCell;

    #[derive(Clone)]
    struct StoredNote {
        format: DocumentFormat,
        capture_id: i64,
        commit_rev: i64,
        document_id: i64,
        document_title: String,
        section_index: u32,
        quote: String,
        note: String,
    }

    struct FakeHost {
        notes: RefCell<Vec<StoredNote>>,
        fail_next_update: RefCell<Option<ApiError>>,
        fail_next_delete: RefCell<bool>,
    }

    impl FakeHost {
        fn new() -> Self {
            Self {
                notes: RefCell::new(vec![
                    StoredNote {
                        format: DocumentFormat::Epub,
                        capture_id: 21,
                        commit_rev: 12,
                        document_id: 2,
                        document_title: "EPUB Architecture Book".into(),
                        section_index: 1,
                        quote: "Orthogonal capabilities decouple read from write.".into(),
                        note: "Initial epub reflection".into(),
                    },
                    StoredNote {
                        format: DocumentFormat::Pdf,
                        capture_id: 11,
                        commit_rev: 10,
                        document_id: 1,
                        document_title: "PDF Spec Paper".into(),
                        section_index: 0,
                        quote: "Local-first storage keeps user notes independent.".into(),
                        note: "Initial pdf thought".into(),
                    },
                ]),
                fail_next_update: RefCell::new(None),
                fail_next_delete: RefCell::new(false),
            }
        }
    }

    impl HostTransport for FakeHost {
        fn call_bytes(&self, request: &[u8]) -> Result<Vec<u8>, SdkError> {
            let req = CoreKnowledgeRequest::from_slice(request)
                .map_err(|err| SdkError::Serialize(err.to_string()))?;
            let result = match req.operation {
                CoreKnowledgeOperation::NotesPage {
                    format,
                    cursor,
                    limit,
                    ..
                } => {
                    let all = self.notes.borrow();
                    let filtered: Vec<&StoredNote> = all
                        .iter()
                        .filter(|item| format.is_none_or(|f| item.format == f))
                        .collect();
                    let offset = cursor
                        .as_deref()
                        .and_then(|c| c.strip_prefix("off:"))
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    let take = usize::from(limit);
                    let slice: Vec<KnowledgeNoteSummary> = filtered
                        .iter()
                        .skip(offset)
                        .take(take)
                        .map(|item| {
                            let quote_preview = preview(&item.quote);
                            let note_preview = preview(&item.note);
                            KnowledgeNoteSummary {
                                format: item.format,
                                capture_id: item.capture_id,
                                commit_rev: item.commit_rev,
                                document_id: item.document_id,
                                document_title: item.document_title.clone(),
                                section_index: item.section_index,
                                quote_bytes: item.quote.len() as u64,
                                quote_truncated: quote_preview.len() < item.quote.len(),
                                quote_preview,
                                note_bytes: item.note.len() as u64,
                                note_truncated: note_preview.len() < item.note.len(),
                                note_preview,
                            }
                        })
                        .collect();
                    let next_cursor = if offset + slice.len() < filtered.len() {
                        Some(format!("off:{}", offset + slice.len()))
                    } else {
                        None
                    };
                    CoreKnowledgeResult::NotesPage {
                        notes: slice,
                        next_cursor,
                    }
                }
                CoreKnowledgeOperation::NoteBody {
                    format,
                    capture_id,
                    field,
                    offset,
                    max_bytes,
                } => {
                    let notes = self.notes.borrow();
                    if let Some(found) = notes
                        .iter()
                        .find(|n| n.format == format && n.capture_id == capture_id)
                    {
                        let full = match field {
                            NoteField::Quote => &found.quote,
                            NoteField::Note => &found.note,
                        };
                        let start = usize::try_from(offset)
                            .unwrap_or(full.len())
                            .min(full.len());
                        let end_limit = (start + usize::from(max_bytes)).min(full.len());
                        let mut end = end_limit;
                        while !full.is_char_boundary(end) {
                            end -= 1;
                        }
                        let chunk_text = full[start..end].to_owned();
                        let next_offset = if end < full.len() {
                            Some(end as u32)
                        } else {
                            None
                        };
                        CoreKnowledgeResult::NoteBody(KnowledgeNoteBody {
                            format,
                            capture_id,
                            commit_rev: found.commit_rev,
                            document_id: found.document_id,
                            field,
                            offset,
                            total_bytes: full.len() as u64,
                            text: chunk_text,
                            next_offset,
                        })
                    } else {
                        CoreKnowledgeResult::Error {
                            error: ApiError::new(ErrorCode::NotFound, "capture not found"),
                        }
                    }
                }
                CoreKnowledgeOperation::UpdateCaptureNote {
                    format,
                    capture_id,
                    note,
                } => {
                    if let Some(err) = self.fail_next_update.borrow_mut().take() {
                        CoreKnowledgeResult::Error { error: err }
                    } else if note.len() > MAX_KNOWLEDGE_NOTE_BYTES {
                        CoreKnowledgeResult::Error {
                            error: ApiError::new(ErrorCode::BudgetExceeded, "note too long"),
                        }
                    } else {
                        let mut notes = self.notes.borrow_mut();
                        if let Some(found) = notes
                            .iter_mut()
                            .find(|n| n.format == format && n.capture_id == capture_id)
                        {
                            found.note = note;
                            found.commit_rev += 1;
                            CoreKnowledgeResult::NoteUpdated {
                                format,
                                capture_id,
                                commit_rev: found.commit_rev,
                            }
                        } else {
                            CoreKnowledgeResult::Error {
                                error: ApiError::new(ErrorCode::NotFound, "capture not found"),
                            }
                        }
                    }
                }
                CoreKnowledgeOperation::DeleteCapture { format, capture_id } => {
                    if *self.fail_next_delete.borrow() {
                        *self.fail_next_delete.borrow_mut() = false;
                        return Err(SdkError::Transport("transient delete error".into()));
                    }
                    let mut notes = self.notes.borrow_mut();
                    let before = notes.len();
                    notes.retain(|n| !(n.format == format && n.capture_id == capture_id));
                    if notes.len() < before {
                        CoreKnowledgeResult::CaptureDeleted {
                            format,
                            capture_id,
                            commit_rev: 99,
                        }
                    } else {
                        CoreKnowledgeResult::Error {
                            error: ApiError::new(ErrorCode::NotFound, "capture not found"),
                        }
                    }
                }
                other => {
                    return Err(SdkError::Transport(format!(
                        "unexpected operation in mock: {other:?}"
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

    fn preview(text: &str) -> String {
        let mut end = text.len().min(64);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].to_owned()
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
            UiComponent::TextInput { value, .. } => value.contains(needle),
            _ => false,
        };
        hit || node.children.iter().any(|child| find_text(child, needle))
    }

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|c| find_node(c, id))
    }

    #[test]
    fn notes_plugin_mixed_list_filter_body_edit_error_and_two_step_delete() {
        let host = FakeHost::new();
        let mut plugin = NotesPlugin::default();

        // 1. Initial render loads mixed PDF + EPUB notes.
        let initial = plugin
            .render(
                &SlotRenderContext {
                    contribution_id: "org.qusor.notes.workspace".into(),
                    mount_id: "slot-1".into(),
                    generation: 1,
                    api_version: Some(HOST_API_V0_8),
                    compact: false,
                },
                &host,
            )
            .unwrap();
        initial.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&initial.root, "EPUB Architecture Book"));
        assert!(find_text(&initial.root, "PDF Spec Paper"));

        // 2. Scope switch to PDF filters out EPUB.
        let SlotUpdate::Tree(pdf_only) = plugin
            .handle_event(
                &event(
                    "notes_scope_tabs",
                    "switch_scope",
                    UiAction::TabChanged {
                        tab_id: "pdf".into(),
                    },
                ),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        pdf_only.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&pdf_only.root, "PDF Spec Paper"));
        assert!(!find_text(&pdf_only.root, "EPUB Architecture Book"));

        // Switch back to All, then search filter by keyword.
        let _ = plugin
            .handle_event(
                &event(
                    "notes_scope_tabs",
                    "switch_scope",
                    UiAction::TabChanged {
                        tab_id: "all".into(),
                    },
                ),
                &host,
            )
            .unwrap();
        let SlotUpdate::Tree(searched) = plugin
            .handle_event(
                &event(
                    "notes_search_input",
                    "search_changed",
                    UiAction::TextChanged {
                        value: "Orthogonal".into(),
                    },
                ),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        searched.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&searched.root, "EPUB Architecture Book"));
        assert!(!find_text(&searched.root, "PDF Spec Paper"));

        // Clear search.
        let _ = plugin
            .handle_event(
                &event("notes_clear_search_btn", "clear_search", UiAction::Press),
                &host,
            )
            .unwrap();

        // 3. Select EPUB note -> loads immutable quote and editable user note separately.
        let SlotUpdate::Tree(selected) = plugin
            .handle_event(
                &event("note_item_epub_21", "select_note:epub:21", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        selected.validate(UiBudgets::default()).unwrap();
        let quote_node = find_node(&selected.root, "detail_quote_text").unwrap();
        assert!(matches!(
            &quote_node.component,
            UiComponent::Text { content } if content.contains("Orthogonal capabilities decouple read from write.")
        ));
        let draft_node = find_node(&selected.root, "note_draft_input").unwrap();
        assert!(matches!(
            &draft_node.component,
            UiComponent::TextInput { value, multiline: true, .. } if value == "Initial epub reflection"
        ));

        // 4. Edit draft and simulate Host error -> must NOT fake success, must show real error and keep draft.
        let _ = plugin
            .handle_event(
                &event(
                    "note_draft_input",
                    "draft_changed",
                    UiAction::TextChanged {
                        value: "Revised user thought".into(),
                    },
                ),
                &host,
            )
            .unwrap();
        *host.fail_next_update.borrow_mut() = Some(ApiError::new(
            ErrorCode::PermissionDenied,
            "core.knowledge.write denied",
        ));
        let SlotUpdate::Tree(err_tree) = plugin
            .handle_event(&event("save_note_btn", "save_note", UiAction::Press), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        err_tree.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&err_tree.root, "core.knowledge.write denied"));
        assert!(!find_text(&err_tree.root, "已保存个人想法"));
        let draft_after_err = find_node(&err_tree.root, "note_draft_input").unwrap();
        assert!(matches!(
            &draft_after_err.component,
            UiComponent::TextInput { value, .. } if value == "Revised user thought"
        ));

        // 5. Save note succeeds -> persisted to host and quote remains unchanged.
        let SlotUpdate::Tree(saved_tree) = plugin
            .handle_event(&event("save_note_btn", "save_note", UiAction::Press), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        saved_tree.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&saved_tree.root, "已保存个人想法"));
        assert!(find_text(
            &saved_tree.root,
            "当前已保存：Revised user thought"
        ));
        assert_eq!(host.notes.borrow()[0].note, "Revised user thought");
        assert_eq!(
            host.notes.borrow()[0].quote,
            "Orthogonal capabilities decouple read from write."
        );

        // 6. Two-step delete: step 1 prompts confirmation without deleting; step 2 deletes.
        let SlotUpdate::Tree(ask_delete) = plugin
            .handle_event(
                &event("delete_note_btn", "request_delete", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        ask_delete.validate(UiBudgets::default()).unwrap();
        assert!(find_node(&ask_delete.root, "confirm_delete_btn").is_some());
        assert_eq!(host.notes.borrow().len(), 2);

        let SlotUpdate::Tree(deleted_tree) = plugin
            .handle_event(
                &event("confirm_delete_btn", "confirm_delete", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        deleted_tree.validate(UiBudgets::default()).unwrap();
        assert_eq!(host.notes.borrow().len(), 1);
        assert!(!find_text(&deleted_tree.root, "EPUB Architecture Book"));
        assert!(find_text(&deleted_tree.root, "PDF Spec Paper"));
    }

    #[test]
    fn notes_plugin_chunked_body_pagination_and_compact_layout() {
        let host = FakeHost::new();
        {
            let mut notes = host.notes.borrow_mut();
            notes.clear();
            for i in 1..=18_i64 {
                notes.push(StoredNote {
                    format: if i % 2 == 0 {
                        DocumentFormat::Epub
                    } else {
                        DocumentFormat::Pdf
                    },
                    capture_id: i,
                    commit_rev: 100 + i,
                    document_id: i,
                    document_title: format!("Doc #{i}"),
                    section_index: 0,
                    quote: if i == 1 {
                        "Q".repeat(2500)
                    } else {
                        format!("Quote {i}")
                    },
                    note: if i == 1 {
                        "N".repeat(2500)
                    } else {
                        format!("Note {i}")
                    },
                });
            }
        }

        let mut plugin = NotesPlugin::default();
        let page1 = plugin
            .render(
                &SlotRenderContext {
                    contribution_id: "org.qusor.notes.workspace".into(),
                    mount_id: "slot-1".into(),
                    generation: 1,
                    api_version: Some(HOST_API_V0_8),
                    compact: true,
                },
                &host,
            )
            .unwrap();
        page1.validate(UiBudgets::default()).unwrap();
        assert!(find_node(&page1.root, "notes_workspace_split").is_none());
        assert!(find_text(&page1.root, "Doc #1"));
        assert!(!find_text(&page1.root, "Doc #17"));

        // Select Doc #1 -> multi-chunk NoteBody (> 2048 bytes) loads completely
        let SlotUpdate::Tree(selected) = plugin
            .handle_event(
                &event("note_item_pdf_1", "select_note:pdf:1", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        selected.validate(UiBudgets::default()).unwrap();
        let quote_node = find_node(&selected.root, "detail_quote_text").unwrap();
        assert!(matches!(
            &quote_node.component,
            UiComponent::Text { content } if content == &"Q".repeat(2500)
        ));
        let draft_node = find_node(&selected.root, "note_draft_input").unwrap();
        assert!(matches!(
            &draft_node.component,
            UiComponent::TextInput { value, .. } if value == &"N".repeat(2500)
        ));

        // Next page -> shows Doc #17 and #18; Prev page -> returns to Doc #1
        let SlotUpdate::Tree(page2) = plugin
            .handle_event(
                &event("notes_next_page_btn", "next_page", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        page2.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&page2.root, "Doc #17"));
        assert!(find_text(&page2.root, "Doc #18"));

        let SlotUpdate::Tree(back) = plugin
            .handle_event(
                &event("notes_prev_page_btn", "prev_page", UiAction::Press),
                &host,
            )
            .unwrap()
        else {
            panic!("expected tree");
        };
        back.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&back.root, "Doc #1"));
    }
}
