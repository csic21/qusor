#![forbid(unsafe_code)]

use qusor_sdk::ui::{
    Align, ControlledSurfaceKind, SemanticToken, SpacingScale, TypographyRole, UiAction,
    UiEventEnvelope, UiNode, UiTree,
};
use qusor_sdk::ui_builder as ui;
use qusor_sdk::{
    DocumentFormat, HostTransport, KnowledgeDocumentSummary, KnowledgeNoteSummary,
    KnowledgeSavedRange, KnowledgeTocEntry, MAX_KNOWLEDGE_PAGE, OrthogonalKnowledgeClient,
    SlotPlugin, SlotRenderContext, SlotUpdate,
};

const MAX_RENDERED_TOC_ITEMS: usize = 80;
const MAX_RENDERED_SECTION_ITEMS: usize = 16;
const MAX_DOC_NOTES_PAGES: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FormatFilter {
    #[default]
    All,
    Pdf,
    Epub,
}

impl FormatFilter {
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

    fn to_doc_format(self) -> Option<DocumentFormat> {
        match self {
            Self::All => None,
            Self::Pdf => Some(DocumentFormat::Pdf),
            Self::Epub => Some(DocumentFormat::Epub),
        }
    }
}

#[derive(Default)]
pub struct ReaderPlugin {
    revision: u64,
    request_seq: u64,
    compact: bool,
    filter: FormatFilter,
    search_query: String,
    documents: Vec<KnowledgeDocumentSummary>,
    next_cursor: Option<String>,
    view: ReaderView,
}

#[derive(Default)]
enum ReaderView {
    #[default]
    Shelf,
    Reading(ActiveReadingState),
}

struct ActiveReadingState {
    document: KnowledgeDocumentSummary,
    section_index: u32,
    toc: Vec<KnowledgeTocEntry>,
    saved_ranges: Vec<KnowledgeSavedRange>,
    doc_notes: Vec<KnowledgeNoteSummary>,
    show_toc: bool,
}

impl ReaderPlugin {
    fn next_req_id(&mut self, prefix: &str) -> String {
        self.request_seq += 1;
        format!("{prefix}-{}", self.request_seq)
    }

    fn load_documents(&mut self, host: &dyn HostTransport, reset: bool) -> Option<String> {
        let cursor = if reset {
            None
        } else {
            self.next_cursor.clone()
        };
        if !reset && cursor.is_none() {
            return None;
        }
        let req_id = self.next_req_id("reader-docs");
        let client = OrthogonalKnowledgeClient::with_transport(host);
        match client.documents_page(
            &req_id,
            self.filter.to_doc_format(),
            cursor.as_deref(),
            MAX_KNOWLEDGE_PAGE,
        ) {
            Ok(page) => {
                if reset {
                    self.documents = page.documents;
                } else {
                    for doc in page.documents {
                        if !self
                            .documents
                            .iter()
                            .any(|existing| same_doc(existing, &doc))
                        {
                            self.documents.push(doc);
                        }
                    }
                }
                self.next_cursor = page.next_cursor;
                None
            }
            Err(err) => {
                if reset {
                    self.documents.clear();
                    self.next_cursor = None;
                }
                Some(err.to_string())
            }
        }
    }

    fn open_document(
        &mut self,
        format: DocumentFormat,
        document_id: i64,
        host: &dyn HostTransport,
    ) -> Option<String> {
        if !self
            .documents
            .iter()
            .any(|doc| doc.format == format && doc.document_id == document_id)
            && let Some(err) = self.load_documents(host, true)
        {
            return Some(err);
        }

        let Some(document) = self
            .documents
            .iter()
            .find(|doc| doc.format == format && doc.document_id == document_id)
            .cloned()
        else {
            return Some(format!("未找到文档 {}:{}", format_tag(format), document_id));
        };

        let toc_req = self.next_req_id("reader-toc");
        let toc = {
            let client = OrthogonalKnowledgeClient::with_transport(host);
            match client.document_toc(&toc_req, format, document_id) {
                Ok(entries) => entries,
                Err(err) => return Some(err.to_string()),
            }
        };

        let mut error = None;
        let saved_ranges = {
            let sel_req = self.next_req_id("reader-sel");
            let client = OrthogonalKnowledgeClient::with_transport(host);
            match client.saved_selections(&sel_req, format, document_id, 0) {
                Ok(ranges) => ranges,
                Err(err) => {
                    error = Some(err.to_string());
                    Vec::new()
                }
            }
        };

        let doc_notes = match self.fetch_document_notes(format, document_id, host) {
            Ok(notes) => notes,
            Err(err) => {
                if error.is_none() {
                    error = Some(err);
                }
                Vec::new()
            }
        };

        let show_toc = !self.compact && !toc.is_empty();
        self.view = ReaderView::Reading(ActiveReadingState {
            document,
            section_index: 0,
            toc,
            saved_ranges,
            doc_notes,
            show_toc,
        });
        error
    }

    fn fetch_document_notes(
        &mut self,
        format: DocumentFormat,
        document_id: i64,
        host: &dyn HostTransport,
    ) -> Result<Vec<KnowledgeNoteSummary>, String> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_DOC_NOTES_PAGES {
            let req_id = self.next_req_id("reader-notes");
            let client = OrthogonalKnowledgeClient::with_transport(host);
            let page = client
                .notes_page(
                    &req_id,
                    Some(format),
                    Some(document_id),
                    cursor.as_deref(),
                    MAX_KNOWLEDGE_PAGE,
                )
                .map_err(|err| err.to_string())?;
            all.extend(page.notes);
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(all)
    }

    fn refresh_active_section(&mut self, host: &dyn HostTransport) -> Option<String> {
        let (format, document_id, section_index) = match &self.view {
            ReaderView::Reading(state) => (
                state.document.format,
                state.document.document_id,
                state.section_index,
            ),
            ReaderView::Shelf => return None,
        };

        let sel_req = self.next_req_id("reader-sel");
        let saved_result = {
            let client = OrthogonalKnowledgeClient::with_transport(host);
            client.saved_selections(&sel_req, format, document_id, section_index)
        };
        let notes_result = self.fetch_document_notes(format, document_id, host);

        let mut error = None;
        if let ReaderView::Reading(state) = &mut self.view {
            match saved_result {
                Ok(ranges) => state.saved_ranges = ranges,
                Err(err) => error = Some(err.to_string()),
            }
            match notes_result {
                Ok(notes) => state.doc_notes = notes,
                Err(err) => {
                    if error.is_none() {
                        error = Some(err);
                    }
                }
            }
        }
        error
    }

    fn refresh_active_document(&mut self, host: &dyn HostTransport) -> Option<String> {
        let (format, document_id) = match &self.view {
            ReaderView::Reading(state) => (state.document.format, state.document.document_id),
            ReaderView::Shelf => return None,
        };
        let toc_req = self.next_req_id("reader-toc");
        let toc_result = {
            let client = OrthogonalKnowledgeClient::with_transport(host);
            client.document_toc(&toc_req, format, document_id)
        };
        let mut error = None;
        if let ReaderView::Reading(state) = &mut self.view {
            match toc_result {
                Ok(toc) => state.toc = toc,
                Err(err) => error = Some(err.to_string()),
            }
        }
        let section_err = self.refresh_active_section(host);
        error.or(section_err)
    }

    fn switch_section(&mut self, target_index: u32, host: &dyn HostTransport) -> Option<String> {
        let max_index = match &self.view {
            ReaderView::Reading(state) => state.document.section_count.saturating_sub(1),
            ReaderView::Shelf => return None,
        };
        let clamped = target_index.min(max_index);
        if let ReaderView::Reading(state) = &mut self.view {
            state.section_index = clamped;
        }
        self.refresh_active_section(host)
    }

    fn filtered_documents(&self) -> Vec<&KnowledgeDocumentSummary> {
        let query = self.search_query.trim().to_lowercase();
        self.documents
            .iter()
            .filter(|doc| {
                self.filter
                    .to_doc_format()
                    .is_none_or(|fmt| doc.format == fmt)
                    && (query.is_empty() || doc.title.to_lowercase().contains(&query))
            })
            .collect()
    }

    fn build_shelf_tree(&mut self, error: Option<&str>) -> UiTree {
        self.revision = self.revision.saturating_add(1).max(1);
        let padding = if self.compact {
            SpacingScale::Sm
        } else {
            SpacingScale::Md
        };

        let header = ui::hstack(
            "reader_shelf_header",
            SpacingScale::Sm,
            Align::Center,
            vec![
                ui::text(
                    "reader_shelf_title",
                    "阅读",
                    TypographyRole::Title,
                    SemanticToken::TextPrimary,
                ),
                ui::badge(
                    "reader_shelf_badge",
                    "Wasm 沙盒插件",
                    SemanticToken::SurfaceAccent,
                ),
                ui::button(
                    "reader_refresh_btn",
                    "刷新书架",
                    "refresh_library",
                    false,
                    false,
                    false,
                ),
            ],
        );

        let filter_tabs = ui::tabs(
            "reader_format_tabs",
            "资料格式筛选",
            self.filter.tab_id(),
            vec![
                ui::tab_option("all", "全部资料"),
                ui::tab_option("pdf", "PDF"),
                ui::tab_option("epub", "EPUB"),
            ],
            "filter_format",
        );

        let mut search_children = vec![ui::with_flex_grow(ui::input(
            "reader_search_input",
            "搜索资料标题",
            self.search_query.clone(),
            "按标题筛选资料…",
            false,
            false,
            "search_changed",
            Some("search_changed".into()),
        ))];
        if !self.search_query.is_empty() {
            search_children.push(ui::button(
                "reader_search_clear_btn",
                "清空",
                "clear_search",
                false,
                false,
                false,
            ));
        }
        let search_row = ui::hstack(
            "reader_search_row",
            SpacingScale::Sm,
            Align::Center,
            search_children,
        );

        let mut children = vec![header, filter_tabs, search_row];
        if let Some(message) = error {
            children.push(error_card("reader_shelf_error", message));
        }

        let filtered = self.filtered_documents();
        if self.documents.is_empty() && self.search_query.trim().is_empty() {
            let desc = match self.filter {
                FormatFilter::All => "请先在资料库中导入 PDF 或 EPUB 文档，随后即可在此打开阅读。",
                FormatFilter::Pdf => "当前知识库尚未导入 PDF 文档。",
                FormatFilter::Epub => "当前知识库尚未导入 EPUB 书籍。",
            };
            children.push(ui::empty_state(
                "reader_shelf_empty",
                "知识库暂无可阅读资料",
                desc,
            ));
        } else if filtered.is_empty() {
            children.push(ui::empty_state(
                "reader_shelf_no_match",
                "没有匹配的资料",
                "请尝试清除搜索词或切换上方格式筛选标签。",
            ));
        } else {
            let items: Vec<UiNode> = filtered
                .into_iter()
                .take(MAX_RENDERED_TOC_ITEMS)
                .map(|doc| {
                    let tag = format_tag(doc.format);
                    let unit = section_unit_label(doc.format);
                    let title = sanitize_singleline(&doc.title, "未命名资料");
                    ui::list_item(
                        format!("doc_item_{tag}_{}", doc.document_id),
                        title,
                        Some(format!(
                            "{} · 共 {} {unit} · rev {}",
                            format_label(doc.format),
                            doc.section_count,
                            doc.commit_rev
                        )),
                        false,
                        format!("open_doc:{tag}:{}", doc.document_id),
                    )
                })
                .collect();
            children.push(ui::section(
                "reader_shelf_list_section",
                "选择资料打开阅读",
                SpacingScale::Sm,
                vec![ui::list("reader_shelf_list", SpacingScale::Xs, items)],
            ));
        }

        if self.next_cursor.is_some() {
            children.push(ui::button(
                "reader_load_more_btn",
                "加载更多资料",
                "load_more_docs",
                false,
                false,
                false,
            ));
        }

        let root = ui::scroll(
            "reader_shelf_scroll",
            padding,
            vec![ui::vstack(
                "reader_shelf_root",
                SpacingScale::Md,
                SpacingScale::None,
                children,
            )],
        );
        ui::ui_tree(self.revision, root)
    }

    fn build_reading_tree(&mut self, error: Option<&str>) -> UiTree {
        self.revision = self.revision.saturating_add(1).max(1);
        let ReaderView::Reading(ref state) = self.view else {
            return ui::ui_tree(
                self.revision,
                ui::empty_state(
                    "reader_invalid_state",
                    "阅读状态异常",
                    "请返回书架重新打开资料。",
                ),
            );
        };

        let padding = if self.compact {
            SpacingScale::Sm
        } else {
            SpacingScale::Md
        };
        let total_sections = state.document.section_count.max(1);
        let current_display = state.section_index.saturating_add(1).min(total_sections);
        let unit = section_unit_label(state.document.format);
        let format_name = format_label(state.document.format);
        let tag = format_tag(state.document.format);
        let doc_title = sanitize_singleline(&state.document.title, "未命名资料");

        let toolbar = ui::hstack(
            "reader_toolbar",
            SpacingScale::Sm,
            Align::Center,
            vec![
                ui::button(
                    "reader_back_btn",
                    "返回书架",
                    "back_to_shelf",
                    false,
                    false,
                    false,
                ),
                ui::with_flex_grow(ui::text(
                    "reader_doc_title",
                    doc_title,
                    TypographyRole::Subtitle,
                    SemanticToken::TextPrimary,
                )),
                ui::badge(
                    "reader_position_badge",
                    format!("{format_name} · 第 {current_display} / {total_sections} {unit}"),
                    SemanticToken::SurfaceAccent,
                ),
                ui::button(
                    "reader_prev_btn",
                    prev_label(state.document.format),
                    "prev_section",
                    false,
                    false,
                    state.section_index == 0,
                ),
                ui::button(
                    "reader_next_btn",
                    next_label(state.document.format),
                    "next_section",
                    true,
                    false,
                    state.section_index.saturating_add(1) >= total_sections,
                ),
                ui::toggle(
                    "reader_toc_toggle",
                    "目录",
                    state.show_toc,
                    false,
                    "toggle_toc",
                ),
                ui::button(
                    "reader_view_refresh_btn",
                    "刷新",
                    "refresh_reader",
                    false,
                    false,
                    false,
                ),
            ],
        );

        let progress = ui::progress("reader_section_progress", current_display, total_sections);

        let surface_id = format!(
            "reader-doc:{tag}:{}:{}",
            state.document.document_id, state.section_index
        );
        let surface_card = ui::card(
            "reader_surface_card",
            SpacingScale::Sm,
            SpacingScale::Sm,
            vec![
                ui::hstack(
                    "reader_surface_header",
                    SpacingScale::Sm,
                    Align::Center,
                    vec![
                        ui::text(
                            "reader_surface_heading",
                            "受控原生阅读视口",
                            TypographyRole::BodyStrong,
                            SemanticToken::TextPrimary,
                        ),
                        ui::badge(
                            "reader_surface_handle_badge",
                            surface_id.clone(),
                            SemanticToken::SurfaceInset,
                        ),
                    ],
                ),
                ui::with_flex_grow(ui::controlled_surface_ref(
                    "reader_document_surface",
                    surface_id,
                    ControlledSurfaceKind::ReaderDocument,
                )),
            ],
        );

        let section_notes: Vec<&KnowledgeNoteSummary> = state
            .doc_notes
            .iter()
            .filter(|note| note.section_index == state.section_index)
            .collect();

        let mut annotation_children = vec![ui::hstack(
            "reader_annotations_header",
            SpacingScale::Sm,
            Align::Center,
            vec![
                ui::text(
                    "reader_annotations_title",
                    "本节高亮与摘录",
                    TypographyRole::BodyStrong,
                    SemanticToken::TextPrimary,
                ),
                ui::badge(
                    "reader_saved_count_badge",
                    format!("已保存高亮 {} 处", state.saved_ranges.len()),
                    SemanticToken::SurfaceAccent,
                ),
                ui::badge(
                    "reader_notes_count_badge",
                    format!(
                        "本节笔记 {} 条 · 全书 {} 条",
                        section_notes.len(),
                        state.doc_notes.len()
                    ),
                    SemanticToken::SurfaceInset,
                ),
            ],
        )];

        if section_notes.is_empty() && state.saved_ranges.is_empty() {
            annotation_children.push(ui::text(
                "reader_annotations_empty",
                "本节暂无已保存的高亮或摘录。",
                TypographyRole::Caption,
                SemanticToken::MutedForeground,
            ));
        } else {
            for (idx, note) in section_notes
                .iter()
                .take(MAX_RENDERED_SECTION_ITEMS)
                .enumerate()
            {
                let quote = sanitize_singleline(&note.quote_preview, "原文摘录");
                let user_note = sanitize_singleline(&note.note_preview, "无附加想法");
                annotation_children.push(ui::key_value(
                    format!("reader_section_note_{idx}"),
                    format!("摘录 #{}：{quote}", note.capture_id),
                    format!("想法：{user_note}"),
                ));
            }
            if section_notes.is_empty() {
                for (idx, range) in state
                    .saved_ranges
                    .iter()
                    .take(MAX_RENDERED_SECTION_ITEMS)
                    .enumerate()
                {
                    annotation_children.push(ui::key_value(
                        format!("reader_saved_range_{idx}"),
                        format!("高亮 #{}", range.capture_id),
                        format!("字符区间 {}..{}", range.start, range.end),
                    ));
                }
            }
        }

        let annotations_card = ui::card(
            "reader_annotations_card",
            SpacingScale::Xs,
            SpacingScale::Sm,
            annotation_children,
        );

        let content_pane = ui::vstack(
            "reader_content_pane",
            SpacingScale::Md,
            SpacingScale::None,
            vec![ui::with_flex_grow(surface_card), annotations_card],
        );

        let body = if state.show_toc {
            let toc_inner = if state.toc.is_empty() {
                ui::empty_state(
                    "reader_toc_empty",
                    "暂无目录",
                    "该文档未提供目录条目，可通过上方按钮切换节次。",
                )
            } else {
                let items: Vec<UiNode> = state
                    .toc
                    .iter()
                    .take(MAX_RENDERED_TOC_ITEMS)
                    .enumerate()
                    .map(|(idx, entry)| {
                        let indent = "  ".repeat(usize::from(entry.depth.min(4)));
                        let title = format!(
                            "{indent}{}",
                            sanitize_singleline(&entry.title, "未命名章节")
                        );
                        ui::list_item(
                            format!("reader_toc_item_{idx}"),
                            title,
                            Some(format!(
                                "第 {} {unit}",
                                entry.section_index.saturating_add(1)
                            )),
                            entry.section_index == state.section_index,
                            format!("jump_section:{}", entry.section_index),
                        )
                    })
                    .collect();
                ui::list("reader_toc_list", SpacingScale::Xs, items)
            };
            let toc_card = ui::card(
                "reader_toc_card",
                SpacingScale::Sm,
                SpacingScale::Sm,
                vec![ui::section(
                    "reader_toc_section",
                    format!("目录 ({})", state.toc.len()),
                    SpacingScale::Xs,
                    vec![toc_inner],
                )],
            );
            ui::with_flex_grow(ui::split(
                "reader_workspace_split",
                28,
                SpacingScale::Md,
                vec![toc_card, content_pane],
            ))
        } else {
            ui::with_flex_grow(content_pane)
        };

        let mut root_children = vec![toolbar, progress];
        if let Some(message) = error {
            root_children.push(error_card("reader_view_error", message));
        }
        root_children.push(body);

        let root = ui::scroll(
            "reader_reading_scroll",
            padding,
            vec![ui::vstack(
                "reader_reading_root",
                SpacingScale::Md,
                SpacingScale::None,
                root_children,
            )],
        );
        ui::ui_tree(self.revision, root)
    }
}

impl SlotPlugin for ReaderPlugin {
    fn render(
        &mut self,
        ctx: &SlotRenderContext,
        host: &dyn HostTransport,
    ) -> Result<UiTree, String> {
        self.compact = ctx.compact;
        match self.view {
            ReaderView::Shelf => {
                let err = self.load_documents(host, true);
                Ok(self.build_shelf_tree(err.as_deref()))
            }
            ReaderView::Reading(_) => {
                let err = self.refresh_active_document(host);
                Ok(self.build_reading_tree(err.as_deref()))
            }
        }
    }

    fn handle_event(
        &mut self,
        event: &UiEventEnvelope,
        host: &dyn HostTransport,
    ) -> Result<SlotUpdate, String> {
        let action_id = event.action_id.as_str();

        if action_id == "back_to_shelf" {
            self.view = ReaderView::Shelf;
            let err = self.load_documents(host, true);
            return Ok(SlotUpdate::Tree(self.build_shelf_tree(err.as_deref())));
        }

        if action_id == "refresh_library" {
            self.view = ReaderView::Shelf;
            let err = self.load_documents(host, true);
            return Ok(SlotUpdate::Tree(self.build_shelf_tree(err.as_deref())));
        }

        if action_id == "load_more_docs" {
            let err = self.load_documents(host, false);
            return Ok(SlotUpdate::Tree(self.build_shelf_tree(err.as_deref())));
        }

        if action_id == "filter_format" {
            if let UiAction::TabChanged { tab_id } = &event.action {
                let Some(next_filter) = FormatFilter::from_tab_id(tab_id) else {
                    return Ok(SlotUpdate::Tree(
                        self.build_shelf_tree(Some("不支持的资料筛选标签")),
                    ));
                };
                self.filter = next_filter;
                let err = self.load_documents(host, true);
                return Ok(SlotUpdate::Tree(self.build_shelf_tree(err.as_deref())));
            }
            return Ok(SlotUpdate::Tree(self.build_shelf_tree(None)));
        }

        if action_id == "search_changed" {
            match &event.action {
                UiAction::TextChanged { value } | UiAction::TextSubmitted { value } => {
                    self.search_query = value.clone();
                }
                _ => {}
            }
            return Ok(SlotUpdate::Tree(self.build_shelf_tree(None)));
        }

        if action_id == "clear_search" {
            self.search_query.clear();
            return Ok(SlotUpdate::Tree(self.build_shelf_tree(None)));
        }

        if let Some(rest) = action_id.strip_prefix("open_doc:") {
            let (format, doc_id) = parse_doc_target(rest)?;
            if let Some(err) = self.open_document(format, doc_id, host) {
                if matches!(self.view, ReaderView::Reading(_)) {
                    return Ok(SlotUpdate::Tree(self.build_reading_tree(Some(&err))));
                }
                return Ok(SlotUpdate::Tree(self.build_shelf_tree(Some(&err))));
            }
            return Ok(SlotUpdate::Tree(self.build_reading_tree(None)));
        }

        if let Some(raw_idx) = action_id.strip_prefix("jump_section:") {
            let index = raw_idx
                .parse::<u32>()
                .map_err(|_| format!("invalid section index `{raw_idx}`"))?;
            let err = self.switch_section(index, host);
            return Ok(SlotUpdate::Tree(self.build_reading_tree(err.as_deref())));
        }

        let err = match action_id {
            "prev_section" => {
                let prev = match &self.view {
                    ReaderView::Reading(state) => state.section_index.saturating_sub(1),
                    ReaderView::Shelf => 0,
                };
                self.switch_section(prev, host)
            }
            "next_section" => {
                let next = match &self.view {
                    ReaderView::Reading(state) => state.section_index.saturating_add(1),
                    ReaderView::Shelf => 0,
                };
                self.switch_section(next, host)
            }
            "toggle_toc" => {
                if let ReaderView::Reading(ref mut state) = self.view {
                    state.show_toc = match event.action {
                        UiAction::Toggled { checked } => checked,
                        _ => !state.show_toc,
                    };
                }
                None
            }
            "refresh_reader" => self.refresh_active_document(host),
            other => Some(format!("未知操作: {other}")),
        };

        let tree = match self.view {
            ReaderView::Shelf => self.build_shelf_tree(err.as_deref()),
            ReaderView::Reading(_) => self.build_reading_tree(err.as_deref()),
        };
        Ok(SlotUpdate::Tree(tree))
    }
}

fn parse_doc_target(raw: &str) -> Result<(DocumentFormat, i64), String> {
    let Some((fmt, id_str)) = raw.split_once(':') else {
        return Err(format!("invalid open_doc target `{raw}`"));
    };
    let format = match fmt {
        "pdf" => DocumentFormat::Pdf,
        "epub" => DocumentFormat::Epub,
        _ => return Err(format!("unknown document format `{fmt}`")),
    };
    let doc_id = id_str
        .parse::<i64>()
        .map_err(|_| format!("invalid document id `{id_str}`"))?;
    Ok((format, doc_id))
}

fn same_doc(a: &KnowledgeDocumentSummary, b: &KnowledgeDocumentSummary) -> bool {
    a.format == b.format && a.document_id == b.document_id
}

fn format_tag(format: DocumentFormat) -> &'static str {
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

fn section_unit_label(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Pdf => "页",
        DocumentFormat::Epub => "章",
    }
}

fn prev_label(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Pdf => "上一页",
        DocumentFormat::Epub => "上一章",
    }
}

fn next_label(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Pdf => "下一页",
        DocumentFormat::Epub => "下一章",
    }
}

fn sanitize_singleline(raw: &str, fallback: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn error_card(id: &str, message: &str) -> UiNode {
    ui::card(
        id,
        SpacingScale::Xs,
        SpacingScale::Sm,
        vec![
            ui::badge(
                format!("{id}_badge"),
                "请求失败",
                SemanticToken::SurfaceDanger,
            ),
            ui::text(
                format!("{id}_text"),
                message.to_owned(),
                TypographyRole::Caption,
                SemanticToken::TextDanger,
            ),
        ],
    )
}
