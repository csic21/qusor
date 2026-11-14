#![forbid(unsafe_code)]

use qusor_sdk::orthogonal::{
    DocumentFormat, KnowledgeNoteSummary, MAX_KNOWLEDGE_CHUNK_BYTES, MAX_KNOWLEDGE_PAGE,
};
use qusor_sdk::ui::{
    Align, SemanticToken, SpacingScale, TypographyRole, UiAction, UiEventEnvelope, UiNode, UiTree,
};
use qusor_sdk::ui_builder as ui;
use qusor_sdk::{
    HostTransport, NoteField, OrthogonalKnowledgeClient, SlotPlugin, SlotRenderContext, SlotUpdate,
};

const MAX_BODY_CHUNKS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ScopeFilter {
    #[default]
    All,
    Pdf,
    Epub,
}

impl ScopeFilter {
    fn from_tab_id(tab_id: &str) -> Option<Self> {
        match tab_id {
            "all" => Some(Self::All),
            "pdf" => Some(Self::Pdf),
            "epub" => Some(Self::Epub),
            _ => None,
        }
    }

    fn tab_id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Pdf => "pdf",
            Self::Epub => "epub",
        }
    }

    fn format_param(self) -> Option<DocumentFormat> {
        match self {
            Self::All => None,
            Self::Pdf => Some(DocumentFormat::Pdf),
            Self::Epub => Some(DocumentFormat::Epub),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectedNote {
    format: DocumentFormat,
    capture_id: i64,
    commit_rev: i64,
    document_id: i64,
    document_title: String,
    section_index: u32,
    quote_text: String,
    quote_truncated: bool,
    note_text: String,
    draft_note: String,
    confirm_delete: bool,
    status_notice: Option<String>,
}

#[derive(Default)]
pub struct NotesPlugin {
    revision: u64,
    request_seq: u64,
    compact: bool,
    scope: ScopeFilter,
    search_query: String,
    current_cursor: Option<String>,
    prev_cursors: Vec<Option<String>>,
    next_cursor: Option<String>,
    notes: Vec<KnowledgeNoteSummary>,
    selected: Option<SelectedNote>,
    list_notice: Option<String>,
    last_error: Option<String>,
}

impl NotesPlugin {
    fn next_req_id(&mut self, prefix: &str) -> String {
        self.request_seq += 1;
        format!("{prefix}-{}", self.request_seq)
    }

    fn load_page(&mut self, host: &dyn HostTransport) -> Option<String> {
        let req_id = self.next_req_id("notes-page");
        let client = OrthogonalKnowledgeClient::with_transport(host);
        match client.notes_page(
            &req_id,
            self.scope.format_param(),
            None,
            self.current_cursor.as_deref(),
            MAX_KNOWLEDGE_PAGE,
        ) {
            Ok(page) => {
                self.notes = page.notes;
                self.next_cursor = page.next_cursor;
                None
            }
            Err(err) => Some(err.to_string()),
        }
    }

    fn fetch_full_field(
        &mut self,
        host: &dyn HostTransport,
        format: DocumentFormat,
        capture_id: i64,
        field: NoteField,
    ) -> Result<(String, i64, i64, bool), String> {
        let mut offset: u32 = 0;
        let mut full = String::new();
        let mut commit_rev = 0;
        let mut document_id = 0;
        let mut truncated = false;

        for chunk_index in 0..MAX_BODY_CHUNKS {
            let req_id = self.next_req_id("notes-body");
            let client = OrthogonalKnowledgeClient::with_transport(host);
            let chunk = client
                .note_body(
                    &req_id,
                    format,
                    capture_id,
                    field,
                    offset,
                    MAX_KNOWLEDGE_CHUNK_BYTES,
                )
                .map_err(|err| err.to_string())?;
            commit_rev = chunk.commit_rev;
            document_id = chunk.document_id;
            full.push_str(&chunk.text);
            match chunk.next_offset {
                Some(next) if next > offset => {
                    if chunk_index + 1 >= MAX_BODY_CHUNKS {
                        truncated = true;
                        break;
                    }
                    offset = next;
                }
                _ => break,
            }
        }
        Ok((
            sanitize_multiline(&full),
            commit_rev,
            document_id,
            truncated,
        ))
    }

    fn select_note(
        &mut self,
        host: &dyn HostTransport,
        format: DocumentFormat,
        capture_id: i64,
    ) -> Option<String> {
        let summary = self
            .notes
            .iter()
            .find(|item| item.format == format && item.capture_id == capture_id)
            .cloned();

        let (quote_text, quote_rev, quote_doc_id, quote_truncated) =
            match self.fetch_full_field(host, format, capture_id, NoteField::Quote) {
                Ok(ok) => ok,
                Err(err) => return Some(err),
            };
        let (note_text, note_rev, note_doc_id, _) =
            match self.fetch_full_field(host, format, capture_id, NoteField::Note) {
                Ok(ok) => ok,
                Err(err) => return Some(err),
            };

        let commit_rev = note_rev.max(quote_rev);
        let document_id = summary
            .as_ref()
            .map(|item| item.document_id)
            .unwrap_or_else(|| note_doc_id.max(quote_doc_id));
        let document_title = summary
            .as_ref()
            .map(|item| item.document_title.clone())
            .unwrap_or_else(|| format!("文档 #{document_id}"));
        let section_index = summary.as_ref().map_or(0, |item| item.section_index);

        self.selected = Some(SelectedNote {
            format,
            capture_id,
            commit_rev,
            document_id,
            document_title,
            section_index,
            quote_text,
            quote_truncated,
            draft_note: note_text.clone(),
            note_text,
            confirm_delete: false,
            status_notice: None,
        });
        self.list_notice = None;
        None
    }

    fn save_selected_note(&mut self, host: &dyn HostTransport) -> Option<String> {
        let Some(ref sel) = self.selected else {
            return Some("请先选择一条想法后再保存。".into());
        };
        let format = sel.format;
        let capture_id = sel.capture_id;
        let note_to_save = sanitize_multiline(&sel.draft_note);

        let req_id = self.next_req_id("notes-update");
        let receipt = {
            let client = OrthogonalKnowledgeClient::with_transport(host);
            client.update_capture_note(&req_id, format, capture_id, &note_to_save)
        };
        let receipt = match receipt {
            Ok(ok) => ok,
            Err(err) => return Some(err.to_string()),
        };

        if let Some(ref mut sel) = self.selected
            && sel.format == format
            && sel.capture_id == capture_id
        {
            sel.commit_rev = receipt.commit_rev;
            sel.note_text = note_to_save.clone();
            sel.draft_note = note_to_save;
            sel.confirm_delete = false;
            sel.status_notice = Some(format!("已保存个人想法（rev {}）", receipt.commit_rev));
        }

        self.load_page(host)
    }

    fn delete_selected_note(&mut self, host: &dyn HostTransport) -> Option<String> {
        let Some(ref sel) = self.selected else {
            return Some("请先选择要删除的想法。".into());
        };
        if !sel.confirm_delete {
            return Some("请先点击「删除想法」，再二次确认执行删除。".into());
        }
        let format = sel.format;
        let capture_id = sel.capture_id;

        let req_id = self.next_req_id("notes-delete");
        let deleted = {
            let client = OrthogonalKnowledgeClient::with_transport(host);
            client.delete_capture(&req_id, format, capture_id)
        };
        if let Err(err) = deleted {
            return Some(err.to_string());
        }

        self.selected = None;
        self.list_notice = Some("已从知识库永久移除该条想法".into());
        let err = self.load_page(host);
        if err.is_none()
            && self.notes.is_empty()
            && let Some(prev) = self.prev_cursors.pop()
        {
            self.current_cursor = prev;
            return self.load_page(host);
        }
        err
    }

    fn filtered_notes(&self) -> Vec<&KnowledgeNoteSummary> {
        let needle = self.search_query.trim().to_lowercase();
        self.notes
            .iter()
            .filter(|item| {
                if needle.is_empty() {
                    return true;
                }
                item.document_title.to_lowercase().contains(&needle)
                    || item.quote_preview.to_lowercase().contains(&needle)
                    || item.note_preview.to_lowercase().contains(&needle)
                    || format_label(item.format).to_lowercase().contains(&needle)
            })
            .collect()
    }

    fn build_tree(&mut self) -> UiTree {
        self.revision = self.revision.saturating_add(1).max(1);
        let filtered = self.filtered_notes();

        let header = ui::hstack(
            "notes_header",
            SpacingScale::Sm,
            Align::Center,
            vec![
                ui::text(
                    "notes_title",
                    "想法",
                    TypographyRole::Title,
                    SemanticToken::TextPrimary,
                ),
                ui::badge(
                    "notes_count_badge",
                    format!("当前页 {} 条", self.notes.len()),
                    SemanticToken::SurfaceAccent,
                ),
                ui::button(
                    "notes_refresh_btn",
                    "刷新",
                    "refresh_notes",
                    false,
                    false,
                    false,
                ),
            ],
        );

        let scope_tabs = ui::tabs(
            "notes_scope_tabs",
            "按来源格式筛选想法",
            self.scope.tab_id(),
            vec![
                ui::tab_option("all", "全部"),
                ui::tab_option("pdf", "PDF"),
                ui::tab_option("epub", "EPUB"),
            ],
            "switch_scope",
        );

        let mut search_children = vec![ui::with_flex_grow(ui::input(
            "notes_search_input",
            "搜索想法",
            self.search_query.clone(),
            "按文档标题、引文或想法关键词过滤…",
            false,
            false,
            "search_changed",
            Some("search_submit".into()),
        ))];
        if !self.search_query.is_empty() {
            search_children.push(ui::button(
                "notes_clear_search_btn",
                "清除",
                "clear_search",
                false,
                false,
                false,
            ));
        }
        let search_row = ui::hstack(
            "notes_search_row",
            SpacingScale::Sm,
            Align::Center,
            search_children,
        );

        let mut root_children = vec![header, scope_tabs, search_row];

        if let Some(ref err) = self.last_error {
            root_children.push(error_card("notes_error_card", err));
        }
        if let Some(ref notice) = self.list_notice {
            root_children.push(ui::badge(
                "notes_list_notice",
                sanitize_singleline(notice, 120),
                SemanticToken::SurfaceInset,
            ));
        }

        let list_pane = self.build_list_pane(&filtered);
        let detail_pane = self.build_detail_pane();

        if self.compact {
            root_children.push(list_pane);
            root_children.push(detail_pane);
        } else {
            root_children.push(ui::split(
                "notes_workspace_split",
                44,
                SpacingScale::Md,
                vec![list_pane, detail_pane],
            ));
        }

        let padding = if self.compact {
            SpacingScale::Sm
        } else {
            SpacingScale::Md
        };
        let root = ui::scroll(
            "notes_workspace_scroll",
            padding,
            vec![ui::vstack(
                "notes_workspace_root",
                SpacingScale::Md,
                SpacingScale::None,
                root_children,
            )],
        );
        ui::ui_tree(self.revision, root)
    }

    fn build_list_pane(&self, filtered: &[&KnowledgeNoteSummary]) -> UiNode {
        let mut children = Vec::new();
        if self.notes.is_empty() {
            children.push(ui::empty_state(
                "notes_list_empty",
                "暂无想法记录",
                "在 PDF 或 EPUB 阅读器中选中文本并记录想法后，会合并展示在这里。",
            ));
        } else if filtered.is_empty() {
            children.push(ui::empty_state(
                "notes_search_empty",
                "没有匹配的想法",
                "当前页没有命中搜索关键词的摘录或想法，可清除搜索或翻页查看。",
            ));
        } else {
            let items: Vec<UiNode> = filtered
                .iter()
                .map(|note| {
                    let slug = format_slug(note.format);
                    let is_selected = self.selected.as_ref().is_some_and(|sel| {
                        sel.format == note.format && sel.capture_id == note.capture_id
                    });
                    let title = sanitize_singleline(
                        &format!("[{}] {}", format_label(note.format), note.document_title),
                        160,
                    );
                    let subtitle = format_list_subtitle(note);
                    ui::list_item(
                        format!("note_item_{slug}_{}", note.capture_id),
                        title,
                        Some(subtitle),
                        is_selected,
                        format!("select_note:{slug}:{}", note.capture_id),
                    )
                })
                .collect();
            children.push(ui::list("notes_list", SpacingScale::Xs, items));
        }

        if !self.prev_cursors.is_empty() || self.next_cursor.is_some() {
            children.push(ui::hstack(
                "notes_pagination_row",
                SpacingScale::Sm,
                Align::Center,
                vec![
                    ui::button(
                        "notes_prev_page_btn",
                        "上一页",
                        "prev_page",
                        false,
                        false,
                        self.prev_cursors.is_empty(),
                    ),
                    ui::button(
                        "notes_next_page_btn",
                        "下一页",
                        "next_page",
                        false,
                        false,
                        self.next_cursor.is_none(),
                    ),
                ],
            ));
        }

        let section_title = if self.search_query.trim().is_empty() {
            format!("全部想法（{}）", filtered.len())
        } else {
            format!("筛选结果（{} / {}）", filtered.len(), self.notes.len())
        };
        ui::section(
            "notes_list_section",
            section_title,
            SpacingScale::Sm,
            children,
        )
    }

    fn build_detail_pane(&self) -> UiNode {
        let Some(ref sel) = self.selected else {
            return ui::section(
                "notes_detail_section",
                "想法详情",
                SpacingScale::Sm,
                vec![ui::empty_state(
                    "notes_detail_empty",
                    "选择一条想法",
                    "从列表点击任意 PDF 或 EPUB 想法，查看完整原文摘录并编辑个人观点。",
                )],
            );
        };

        let unit_label = match sel.format {
            DocumentFormat::Pdf => format!("第 {} 页", sel.section_index + 1),
            DocumentFormat::Epub => format!("第 {} 章", sel.section_index + 1),
        };
        let meta_row = ui::hstack(
            "detail_meta_row",
            SpacingScale::Sm,
            Align::Center,
            vec![
                ui::badge(
                    "detail_format_badge",
                    format_label(sel.format),
                    SemanticToken::SurfaceAccent,
                ),
                ui::badge(
                    "detail_rev_badge",
                    format!("记录 #{} · rev {}", sel.capture_id, sel.commit_rev),
                    SemanticToken::SurfaceInset,
                ),
                ui::button(
                    "detail_close_btn",
                    "关闭详情",
                    "clear_selection",
                    false,
                    false,
                    false,
                ),
            ],
        );

        let quote_body = if sel.quote_text.trim().is_empty() {
            "（该条记录未附带原文引文）".to_owned()
        } else if sel.quote_truncated {
            format!("{}\n…（原文引文较长，已截断显示）", sel.quote_text)
        } else {
            sel.quote_text.clone()
        };

        let quote_card = ui::card(
            "detail_quote_card",
            SpacingScale::Xs,
            SpacingScale::Sm,
            vec![
                ui::hstack(
                    "detail_quote_header",
                    SpacingScale::Sm,
                    Align::Center,
                    vec![
                        ui::badge(
                            "detail_quote_badge",
                            "原文引文（不可编辑）",
                            SemanticToken::SurfaceInset,
                        ),
                        ui::text(
                            "detail_source_loc",
                            format!("{} · {}", sel.document_title, unit_label),
                            TypographyRole::Caption,
                            SemanticToken::MutedForeground,
                        ),
                    ],
                ),
                ui::text(
                    "detail_quote_text",
                    quote_body,
                    TypographyRole::Body,
                    SemanticToken::TextSecondary,
                ),
            ],
        );

        let saved_display = if sel.note_text.trim().is_empty() {
            "（暂无已保存想法）".to_owned()
        } else {
            sel.note_text.clone()
        };

        let mut action_buttons = vec![
            ui::button("save_note_btn", "保存想法", "save_note", true, false, false),
            ui::button(
                "reset_draft_btn",
                "重置草稿",
                "reset_draft",
                false,
                false,
                sel.draft_note == sel.note_text,
            ),
        ];
        if sel.confirm_delete {
            action_buttons.push(ui::button(
                "confirm_delete_btn",
                "确认删除",
                "confirm_delete",
                false,
                true,
                false,
            ));
            action_buttons.push(ui::button(
                "cancel_delete_btn",
                "取消删除",
                "cancel_delete",
                false,
                false,
                false,
            ));
        } else {
            action_buttons.push(ui::button(
                "delete_note_btn",
                "删除想法",
                "request_delete",
                false,
                true,
                false,
            ));
        }

        let mut thought_children = vec![
            ui::hstack(
                "detail_thought_header",
                SpacingScale::Sm,
                Align::Center,
                vec![
                    ui::badge(
                        "detail_thought_badge",
                        "用户观点（可编辑）",
                        SemanticToken::SurfaceAccent,
                    ),
                    ui::key_value("detail_doc_id", "文档", format!("#{}", sel.document_id)),
                ],
            ),
            ui::text(
                "detail_saved_note_text",
                format!("当前已保存：{saved_display}"),
                TypographyRole::Caption,
                SemanticToken::MutedForeground,
            ),
            ui::input(
                "note_draft_input",
                "编辑个人想法",
                sel.draft_note.clone(),
                "输入或修改你的个人想法（支持多行）…",
                true,
                false,
                "draft_changed",
                Some("save_note".into()),
            ),
            ui::hstack(
                "detail_actions_row",
                SpacingScale::Sm,
                Align::Center,
                action_buttons,
            ),
        ];

        if sel.confirm_delete {
            thought_children.push(ui::text(
                "delete_warning_text",
                "确认后将从 Core 永久删除此条捕获记录与想法，操作不可撤销。",
                TypographyRole::Caption,
                SemanticToken::TextDanger,
            ));
        }
        if let Some(ref notice) = sel.status_notice {
            thought_children.push(ui::badge(
                "detail_status_badge",
                sanitize_singleline(notice, 120),
                SemanticToken::SurfaceAccent,
            ));
        }

        let thought_card = ui::card(
            "detail_thought_card",
            SpacingScale::Sm,
            SpacingScale::Sm,
            thought_children,
        );

        ui::section(
            "notes_detail_section",
            "原文摘录与个人想法",
            SpacingScale::Sm,
            vec![
                meta_row,
                quote_card,
                ui::divider("detail_quote_thought_divider"),
                thought_card,
            ],
        )
    }
}

impl SlotPlugin for NotesPlugin {
    fn render(
        &mut self,
        ctx: &SlotRenderContext,
        host: &dyn HostTransport,
    ) -> Result<UiTree, String> {
        self.compact = ctx.compact;
        self.last_error = self.load_page(host);
        if let Some((fmt, cap_id)) = self.selected.as_ref().map(|s| (s.format, s.capture_id))
            && self.last_error.is_none()
            && let Some(err) = self.select_note(host, fmt, cap_id)
        {
            self.selected = None;
            self.last_error = Some(err);
        }
        Ok(self.build_tree())
    }

    fn handle_event(
        &mut self,
        event: &UiEventEnvelope,
        host: &dyn HostTransport,
    ) -> Result<SlotUpdate, String> {
        self.last_error = None;
        let action_id = event.action_id.as_str();

        let error = match (action_id, &event.action) {
            ("refresh_notes", UiAction::Press) => {
                self.list_notice = None;
                let err = self.load_page(host);
                if err.is_none()
                    && let Some((fmt, cap_id)) =
                        self.selected.as_ref().map(|s| (s.format, s.capture_id))
                    && self
                        .notes
                        .iter()
                        .any(|n| n.format == fmt && n.capture_id == cap_id)
                {
                    self.select_note(host, fmt, cap_id)
                } else {
                    err
                }
            }
            ("switch_scope", UiAction::TabChanged { tab_id }) => {
                let Some(next_scope) = ScopeFilter::from_tab_id(tab_id) else {
                    return Ok(SlotUpdate::Tree(
                        self.build_with_error(format!("未知筛选标签: {tab_id}")),
                    ));
                };
                self.scope = next_scope;
                self.current_cursor = None;
                self.prev_cursors.clear();
                self.list_notice = None;
                self.load_page(host)
            }
            ("search_changed", UiAction::TextChanged { value })
            | ("search_submit", UiAction::TextSubmitted { value }) => {
                self.search_query = sanitize_singleline(value, 256);
                None
            }
            ("clear_search", UiAction::Press) => {
                self.search_query.clear();
                None
            }
            ("next_page", UiAction::Press) => {
                if let Some(next) = self.next_cursor.clone() {
                    let prev = self.current_cursor.take();
                    self.prev_cursors.push(prev);
                    self.current_cursor = Some(next);
                    self.load_page(host)
                } else {
                    None
                }
            }
            ("prev_page", UiAction::Press) => {
                if let Some(prev) = self.prev_cursors.pop() {
                    self.current_cursor = prev;
                    self.load_page(host)
                } else {
                    None
                }
            }
            ("clear_selection", UiAction::Press) => {
                self.selected = None;
                None
            }
            ("draft_changed", UiAction::TextChanged { value }) => {
                let cleaned = sanitize_multiline(value);
                if let Some(ref mut sel) = self.selected {
                    sel.draft_note = cleaned;
                    sel.status_notice = None;
                    sel.confirm_delete = false;
                }
                None
            }
            ("save_note", UiAction::TextSubmitted { value }) => {
                let cleaned = sanitize_multiline(value);
                if let Some(ref mut sel) = self.selected {
                    sel.draft_note = cleaned;
                }
                self.save_selected_note(host)
            }
            ("save_note", UiAction::Press) => self.save_selected_note(host),
            ("reset_draft", UiAction::Press) => {
                if let Some(ref mut sel) = self.selected {
                    sel.draft_note = sel.note_text.clone();
                    sel.confirm_delete = false;
                    sel.status_notice = None;
                }
                None
            }
            ("request_delete", UiAction::Press) => {
                if let Some(ref mut sel) = self.selected {
                    sel.confirm_delete = true;
                    sel.status_notice = None;
                }
                None
            }
            ("cancel_delete", UiAction::Press) => {
                if let Some(ref mut sel) = self.selected {
                    sel.confirm_delete = false;
                }
                None
            }
            ("confirm_delete", UiAction::Press) => self.delete_selected_note(host),
            (other, UiAction::Press) if other.starts_with("select_note:") => {
                match parse_select_action(other) {
                    Ok((format, capture_id)) => self.select_note(host, format, capture_id),
                    Err(err) => Some(err),
                }
            }
            (other, _) => Some(format!("未知操作: {other}")),
        };

        self.last_error = error;
        Ok(SlotUpdate::Tree(self.build_tree()))
    }
}

impl NotesPlugin {
    fn build_with_error(&mut self, error: String) -> UiTree {
        self.last_error = Some(error);
        self.build_tree()
    }
}

fn parse_select_action(action_id: &str) -> Result<(DocumentFormat, i64), String> {
    let rest = action_id
        .strip_prefix("select_note:")
        .ok_or_else(|| format!("invalid select action `{action_id}`"))?;
    let (fmt_raw, id_raw) = rest
        .split_once(':')
        .ok_or_else(|| format!("invalid select action `{action_id}`"))?;
    let format = match fmt_raw {
        "pdf" => DocumentFormat::Pdf,
        "epub" => DocumentFormat::Epub,
        _ => return Err(format!("unknown note format `{fmt_raw}`")),
    };
    let capture_id = id_raw
        .parse::<i64>()
        .map_err(|_| format!("invalid capture id `{id_raw}`"))?;
    Ok((format, capture_id))
}

fn format_slug(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Pdf => "pdf",
        DocumentFormat::Epub => "epub",
    }
}

fn format_label(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Pdf => "PDF",
        DocumentFormat::Epub => "EPUB",
    }
}

fn format_list_subtitle(note: &KnowledgeNoteSummary) -> String {
    let section_unit = match note.format {
        DocumentFormat::Pdf => "页",
        DocumentFormat::Epub => "章",
    };
    let quote = if note.quote_preview.trim().is_empty() {
        "（无引文）".to_owned()
    } else {
        sanitize_singleline(&note.quote_preview, 48)
    };
    let thought = if note.note_preview.trim().is_empty() {
        "（无想法）".to_owned()
    } else {
        sanitize_singleline(&note.note_preview, 48)
    };
    sanitize_singleline(
        &format!(
            "第 {} {section_unit} · 引文: {quote} · 想法: {thought}",
            note.section_index + 1
        ),
        180,
    )
}

fn sanitize_singleline(input: &str, max_bytes: usize) -> String {
    let mut out = String::with_capacity(input.len().min(max_bytes));
    for ch in input.chars() {
        let mapped = match ch {
            '\n' | '\r' | '\t' => ' ',
            c if c.is_control() => continue,
            c => c,
        };
        if out.len() + mapped.len_utf8() > max_bytes {
            break;
        }
        out.push(mapped);
    }
    out
}

fn sanitize_multiline(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(normalized.len());
    for ch in normalized.chars() {
        if ch == '\t' {
            out.push(' ');
        } else if !ch.is_control() || ch == '\n' {
            out.push(ch);
        }
    }
    out
}

fn error_card(id: &str, message: &str) -> UiNode {
    ui::card(
        id,
        SpacingScale::Xs,
        SpacingScale::Sm,
        vec![
            ui::badge(
                format!("{id}_badge"),
                "操作失败",
                SemanticToken::SurfaceDanger,
            ),
            ui::text(
                format!("{id}_text"),
                sanitize_singleline(message, 400),
                TypographyRole::Caption,
                SemanticToken::TextDanger,
            ),
        ],
    )
}
