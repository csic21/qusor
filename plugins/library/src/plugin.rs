#![forbid(unsafe_code)]

use qusor_sdk::orthogonal::{
    DocumentFormat, KnowledgeDocumentSummary, KnowledgePdfMetadata, KnowledgeSavedRange,
    KnowledgeTocEntry, MAX_KNOWLEDGE_PAGE,
};
use qusor_sdk::ui::{
    Align, ControlledSurfaceKind, SemanticToken, SpacingScale, TypographyRole, UiAction,
    UiEventEnvelope, UiNode, UiTree,
};
use qusor_sdk::ui_builder as ui;
use qusor_sdk::{
    HostTransport, OrthogonalKnowledgeClient, SlotPlugin, SlotRenderContext, SlotUpdate,
};

const MAX_VISIBLE_TOC_ENTRIES: usize = 48;
const MAX_VISIBLE_SAVED_RANGES: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FormatFilter {
    #[default]
    All,
    Pdf,
    Epub,
}

impl FormatFilter {
    fn tab_id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Pdf => "pdf",
            Self::Epub => "epub",
        }
    }

    fn from_tab_id(tab_id: &str) -> Option<Self> {
        match tab_id {
            "all" => Some(Self::All),
            "pdf" => Some(Self::Pdf),
            "epub" => Some(Self::Epub),
            _ => None,
        }
    }

    fn query_format(self) -> Option<DocumentFormat> {
        match self {
            Self::All => None,
            Self::Pdf => Some(DocumentFormat::Pdf),
            Self::Epub => Some(DocumentFormat::Epub),
        }
    }
}

#[derive(Clone, Debug)]
struct DetailState {
    doc: KnowledgeDocumentSummary,
    section_index: u32,
    toc: Vec<KnowledgeTocEntry>,
    saved_ranges: Vec<KnowledgeSavedRange>,
    pdf_meta: Option<KnowledgePdfMetadata>,
    confirming_delete: bool,
}

#[derive(Default)]
enum LibraryView {
    #[default]
    List,
    Detail(Box<DetailState>),
}

#[derive(Default)]
pub struct LibraryPlugin {
    revision: u64,
    request_seq: u64,
    compact: bool,
    filter: FormatFilter,
    search_query: String,
    current_cursor: Option<String>,
    prev_cursors: Vec<Option<String>>,
    next_cursor: Option<String>,
    documents: Vec<KnowledgeDocumentSummary>,
    status_notice: Option<String>,
    view: LibraryView,
}

impl LibraryPlugin {
    fn next_req_id(&mut self, prefix: &str) -> String {
        self.request_seq += 1;
        format!("{prefix}-{}", self.request_seq)
    }

    fn load_page(&mut self, host: &dyn HostTransport) -> Option<String> {
        let req_id = self.next_req_id("lib-docs");
        let client = OrthogonalKnowledgeClient::with_transport(host);
        let format = self.filter.query_format();
        let cursor = self.current_cursor.clone();
        match client.documents_page(&req_id, format, cursor.as_deref(), MAX_KNOWLEDGE_PAGE) {
            Ok(page) => {
                self.documents = page.documents;
                self.next_cursor = page.next_cursor;
                None
            }
            Err(err) => {
                self.documents.clear();
                self.next_cursor = None;
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
        let doc = if let Some(found) = self
            .documents
            .iter()
            .find(|d| d.format == format && d.document_id == document_id)
            .cloned()
        {
            found
        } else if let Some(err) = self.load_page(host) {
            return Some(err);
        } else if let Some(found) = self
            .documents
            .iter()
            .find(|d| d.format == format && d.document_id == document_id)
            .cloned()
        {
            found
        } else {
            return Some(format!("未找到资料 #{}", document_id));
        };

        self.status_notice = None;
        let mut detail = DetailState {
            doc,
            section_index: 0,
            toc: Vec::new(),
            saved_ranges: Vec::new(),
            pdf_meta: None,
            confirming_delete: false,
        };
        let err = self.refresh_detail_data(&mut detail, host);
        self.view = LibraryView::Detail(Box::new(detail));
        err
    }

    fn refresh_detail_data(
        &mut self,
        detail: &mut DetailState,
        host: &dyn HostTransport,
    ) -> Option<String> {
        let toc_req = self.next_req_id("lib-toc");
        let sel_req = self.next_req_id("lib-sel");
        let meta_req = if detail.doc.format == DocumentFormat::Pdf {
            Some(self.next_req_id("lib-meta"))
        } else {
            None
        };

        let client = OrthogonalKnowledgeClient::with_transport(host);
        let mut first_error = None;

        match client.document_toc(&toc_req, detail.doc.format, detail.doc.document_id) {
            Ok(entries) => detail.toc = entries,
            Err(err) => {
                detail.toc.clear();
                first_error = Some(err.to_string());
            }
        }

        match client.saved_selections(
            &sel_req,
            detail.doc.format,
            detail.doc.document_id,
            detail.section_index,
        ) {
            Ok(ranges) => detail.saved_ranges = ranges,
            Err(err) => {
                detail.saved_ranges.clear();
                if first_error.is_none() {
                    first_error = Some(err.to_string());
                }
            }
        }

        if let Some(meta_req) = meta_req {
            match client.pdf_metadata(&meta_req, detail.doc.document_id) {
                Ok(meta) => detail.pdf_meta = Some(meta),
                Err(_) => detail.pdf_meta = None,
            }
        } else {
            detail.pdf_meta = None;
        }

        first_error
    }

    fn refresh_current_detail(&mut self, host: &dyn HostTransport) -> Option<String> {
        let LibraryView::Detail(mut detail) = std::mem::take(&mut self.view) else {
            return None;
        };
        let err = self.refresh_detail_data(&mut detail, host);
        self.view = LibraryView::Detail(detail);
        err
    }

    fn delete_active_document(&mut self, host: &dyn HostTransport) -> Option<String> {
        let (format, document_id, title) = match &self.view {
            LibraryView::Detail(detail) => (
                detail.doc.format,
                detail.doc.document_id,
                detail.doc.title.clone(),
            ),
            LibraryView::List => return Some("当前未选中资料".to_owned()),
        };

        let req_id = self.next_req_id("lib-del");
        let client = OrthogonalKnowledgeClient::with_transport(host);
        match client.delete_document(&req_id, format, document_id) {
            Ok(_) => {
                self.view = LibraryView::List;
                self.current_cursor = None;
                self.prev_cursors.clear();
                self.status_notice = Some(format!("已删除《{}》", singleline(&title)));
                self.load_page(host)
            }
            Err(err) => Some(err.to_string()),
        }
    }

    fn filtered_documents(&self) -> Vec<&KnowledgeDocumentSummary> {
        let needle = self.search_query.trim().to_lowercase();
        self.documents
            .iter()
            .filter(|doc| {
                if needle.is_empty() {
                    return true;
                }
                doc.title.to_lowercase().contains(&needle)
                    || format_label(doc.format).to_lowercase().contains(&needle)
                    || doc.document_id.to_string().contains(&needle)
            })
            .collect()
    }

    fn build_list_tree(&mut self, error: Option<&str>) -> UiTree {
        self.revision = self.revision.saturating_add(1).max(1);
        let padding = if self.compact {
            SpacingScale::Sm
        } else {
            SpacingScale::Md
        };

        let mut children = vec![
            ui::hstack(
                "library_header",
                SpacingScale::Sm,
                Align::Center,
                vec![
                    ui::text(
                        "library_title",
                        "资料库",
                        TypographyRole::Title,
                        SemanticToken::TextPrimary,
                    ),
                    ui::badge(
                        "library_badge",
                        "Wasm 正交插件",
                        SemanticToken::SurfaceAccent,
                    ),
                    ui::button(
                        "import_pdf_btn",
                        "导入 PDF",
                        "host_import_pdf",
                        true,
                        false,
                        false,
                    ),
                    ui::button(
                        "import_epub_btn",
                        "导入 EPUB",
                        "host_import_epub",
                        false,
                        false,
                        false,
                    ),
                    ui::button(
                        "refresh_docs_btn",
                        "刷新",
                        "refresh_docs",
                        false,
                        false,
                        false,
                    ),
                ],
            ),
            ui::tabs(
                "library_format_tabs",
                "资料格式筛选",
                self.filter.tab_id(),
                vec![
                    ui::tab_option("all", "全部"),
                    ui::tab_option("pdf", "PDF"),
                    ui::tab_option("epub", "EPUB"),
                ],
                "filter_format",
            ),
            ui::input(
                "library_search_input",
                "搜索资料标题",
                self.search_query.clone(),
                "按标题或编号过滤已导入 PDF / EPUB...",
                false,
                false,
                "search_changed",
                Some("search_submit".to_owned()),
            ),
        ];

        if let Some(notice) = &self.status_notice {
            children.push(info_card("library_status_card", notice));
        }

        if let Some(message) = error {
            children.push(error_card("library_error_card", message));
        }

        let visible = self.filtered_documents();
        if self.documents.is_empty() {
            children.push(ui::empty_state(
                "library_empty_state",
                "资料库为空",
                "尚未导入 PDF 或 EPUB 文档。请点击上方「导入 PDF」或「导入 EPUB」添加资料。",
            ));
        } else if visible.is_empty() {
            children.push(ui::empty_state(
                "library_search_empty",
                "无匹配资料",
                format!(
                    "当前筛选条件（{}）下没有匹配「{}」的资料。",
                    filter_label(self.filter),
                    singleline(&self.search_query)
                ),
            ));
        } else {
            let items: Vec<UiNode> = visible
                .iter()
                .map(|doc| {
                    let fmt_slug = format_slug(doc.format);
                    let unit = section_unit(doc.format);
                    ui::list_item(
                        format!("doc_item_{fmt_slug}_{}", doc.document_id),
                        singleline(&doc.title),
                        Some(format!(
                            "{} · {} {unit} · rev {}",
                            format_label(doc.format),
                            doc.section_count,
                            doc.commit_rev
                        )),
                        false,
                        format!("open_doc:{fmt_slug}:{}", doc.document_id),
                    )
                })
                .collect();
            children.push(ui::section(
                "library_docs_section",
                format!("已导入资料（本页 {} 项）", visible.len()),
                SpacingScale::Sm,
                vec![ui::list("library_docs_list", SpacingScale::Xs, items)],
            ));
        }

        if !self.prev_cursors.is_empty() || self.next_cursor.is_some() {
            children.push(ui::hstack(
                "library_pagination_row",
                SpacingScale::Sm,
                Align::Center,
                vec![
                    ui::button(
                        "docs_prev_page_btn",
                        "上一页",
                        "prev_page",
                        false,
                        false,
                        self.prev_cursors.is_empty(),
                    ),
                    ui::badge(
                        "docs_page_badge",
                        format!("第 {} 页", self.prev_cursors.len() + 1),
                        SemanticToken::SurfaceInset,
                    ),
                    ui::button(
                        "docs_next_page_btn",
                        "下一页",
                        "next_page",
                        false,
                        false,
                        self.next_cursor.is_none(),
                    ),
                ],
            ));
        }

        let root = ui::scroll(
            "library_list_scroll",
            padding,
            vec![ui::vstack(
                "library_list_root",
                SpacingScale::Md,
                SpacingScale::None,
                children,
            )],
        );
        ui::ui_tree(self.revision, root)
    }

    fn build_detail_tree(&mut self, error: Option<&str>) -> UiTree {
        self.revision = self.revision.saturating_add(1).max(1);
        let LibraryView::Detail(ref state) = self.view else {
            return ui::ui_tree(
                self.revision,
                ui::empty_state("library_invalid_detail", "状态异常", "请返回资料列表重试。"),
            );
        };

        let fmt_slug = format_slug(state.doc.format);
        let unit = section_unit(state.doc.format);
        let total_sections = state.doc.section_count.max(1);
        let current_display = state.section_index.saturating_add(1).min(total_sections);
        let surface_id = format!(
            "library-doc:{fmt_slug}:{}:{}",
            state.doc.document_id, state.section_index
        );

        let mut header_actions = vec![
            ui::button(
                "detail_back_btn",
                "返回资料库",
                "back_to_list",
                false,
                false,
                false,
            ),
            ui::text(
                "detail_doc_title",
                singleline(&state.doc.title),
                TypographyRole::Title,
                SemanticToken::TextPrimary,
            ),
            ui::badge(
                "detail_format_badge",
                format_label(state.doc.format),
                SemanticToken::SurfaceAccent,
            ),
            ui::button(
                "detail_refresh_btn",
                "刷新",
                "refresh_doc",
                false,
                false,
                false,
            ),
        ];
        if !state.confirming_delete {
            header_actions.push(ui::button(
                "detail_delete_btn",
                "删除资料",
                "request_delete",
                false,
                true,
                false,
            ));
        }

        let mut top_nodes = vec![ui::hstack(
            "detail_header",
            SpacingScale::Sm,
            Align::Center,
            header_actions,
        )];

        if let Some(message) = error {
            top_nodes.push(error_card("detail_error_card", message));
        }

        if state.confirming_delete {
            top_nodes.push(ui::card(
                "delete_confirm_card",
                SpacingScale::Sm,
                SpacingScale::Md,
                vec![
                    ui::badge(
                        "delete_confirm_badge",
                        "危险操作 · 两步确认",
                        SemanticToken::SurfaceDanger,
                    ),
                    ui::text(
                        "delete_confirm_text",
                        format!(
                            "确认永久删除「{}」？此操作会移除文档及其所有摘录记录。",
                            singleline(&state.doc.title)
                        ),
                        TypographyRole::Body,
                        SemanticToken::TextDanger,
                    ),
                    ui::hstack(
                        "delete_confirm_actions",
                        SpacingScale::Sm,
                        Align::Center,
                        vec![
                            ui::button(
                                "confirm_delete_btn",
                                "确认删除",
                                "confirm_delete",
                                false,
                                true,
                                false,
                            ),
                            ui::button(
                                "cancel_delete_btn",
                                "取消",
                                "cancel_delete",
                                false,
                                false,
                                false,
                            ),
                        ],
                    ),
                ],
            ));
        }

        top_nodes.push(ui::hstack(
            "detail_nav_bar",
            SpacingScale::Sm,
            Align::Center,
            vec![
                ui::button(
                    "prev_section_btn",
                    if state.doc.format == DocumentFormat::Pdf {
                        "上一页"
                    } else {
                        "上一章"
                    },
                    "prev_section",
                    false,
                    false,
                    state.section_index == 0,
                ),
                ui::badge(
                    "section_progress_badge",
                    format!("第 {current_display}/{total_sections} {unit}"),
                    SemanticToken::SurfaceInset,
                ),
                ui::button(
                    "next_section_btn",
                    if state.doc.format == DocumentFormat::Pdf {
                        "下一页"
                    } else {
                        "下一章"
                    },
                    "next_section",
                    false,
                    false,
                    state.section_index.saturating_add(1) >= total_sections,
                ),
                ui::badge(
                    "saved_selections_badge",
                    format!("本{unit}高亮 {} 条", state.saved_ranges.len()),
                    SemanticToken::SurfaceAccent,
                ),
            ],
        ));

        let mut meta_nodes = vec![
            ui::key_value(
                "detail_doc_id_kv",
                "文档编号",
                format!(
                    "{} #{}",
                    format_label(state.doc.format),
                    state.doc.document_id
                ),
            ),
            ui::key_value(
                "detail_commit_kv",
                "修订版本",
                format!("rev {}", state.doc.commit_rev),
            ),
            ui::key_value(
                "detail_highlights_kv",
                "当前节保存高亮",
                format!("{} 条", state.saved_ranges.len()),
            ),
        ];
        if let Some(ref meta) = state.pdf_meta {
            meta_nodes.push(ui::key_value(
                "detail_pdf_bytes_kv",
                "原文件体积",
                format!("{} 字节", meta.original_bytes),
            ));
        }

        let mut sidebar_children = vec![ui::section(
            "detail_meta_section",
            "文档信息",
            SpacingScale::Xs,
            meta_nodes,
        )];

        if state.toc.is_empty() {
            sidebar_children.push(ui::section(
                "detail_toc_section",
                "目录 TOC",
                SpacingScale::Xs,
                vec![ui::text(
                    "detail_toc_empty",
                    "当前文档暂无目录条目，可通过上方翻页控件切换。",
                    TypographyRole::Caption,
                    SemanticToken::MutedForeground,
                )],
            ));
        } else {
            let toc_items: Vec<UiNode> = state
                .toc
                .iter()
                .take(MAX_VISIBLE_TOC_ENTRIES)
                .enumerate()
                .map(|(idx, entry)| {
                    let indent = "  ".repeat(usize::from(entry.depth.min(4)));
                    ui::list_item(
                        format!("toc_item_{idx}"),
                        singleline(&format!("{indent}{}", entry.title)),
                        Some(format!(
                            "第 {} {unit}",
                            entry.section_index.saturating_add(1)
                        )),
                        entry.section_index == state.section_index,
                        format!("select_section:{}", entry.section_index),
                    )
                })
                .collect();
            sidebar_children.push(ui::section(
                "detail_toc_section",
                format!("目录 TOC（{} 项）", state.toc.len()),
                SpacingScale::Xs,
                vec![ui::list("detail_toc_list", SpacingScale::Xs, toc_items)],
            ));
        }

        if state.saved_ranges.is_empty() {
            sidebar_children.push(ui::section(
                "detail_highlights_section",
                "本节保存选区",
                SpacingScale::Xs,
                vec![ui::text(
                    "detail_highlights_empty",
                    "当前页/章暂无已保存选区高亮。",
                    TypographyRole::Caption,
                    SemanticToken::MutedForeground,
                )],
            ));
        } else {
            let range_nodes: Vec<UiNode> = state
                .saved_ranges
                .iter()
                .take(MAX_VISIBLE_SAVED_RANGES)
                .map(|range| {
                    ui::key_value(
                        format!("saved_range_{}", range.capture_id),
                        format!("摘录 #{}", range.capture_id),
                        format!("区间 {}..{}", range.start, range.end),
                    )
                })
                .collect();
            sidebar_children.push(ui::section(
                "detail_highlights_section",
                format!("本节保存选区（{} 条）", state.saved_ranges.len()),
                SpacingScale::Xs,
                range_nodes,
            ));
        }

        let reader_pane = ui::with_flex_grow(ui::card(
            "detail_reader_card",
            SpacingScale::Sm,
            SpacingScale::Md,
            vec![
                ui::text(
                    "detail_surface_caption",
                    format!("受控原生阅读/选区表面：{surface_id}"),
                    TypographyRole::Caption,
                    SemanticToken::TextSecondary,
                ),
                ui::controlled_surface_ref(
                    "library_reader_surface",
                    surface_id,
                    ControlledSurfaceKind::ReaderDocument,
                ),
            ],
        ));

        let body_node = if self.compact {
            ui::vstack(
                "detail_compact_body",
                SpacingScale::Md,
                SpacingScale::None,
                vec![
                    reader_pane,
                    ui::vstack(
                        "detail_compact_sidebar",
                        SpacingScale::Sm,
                        SpacingScale::None,
                        sidebar_children,
                    ),
                ],
            )
        } else {
            ui::split(
                "detail_split_body",
                36,
                SpacingScale::Md,
                vec![
                    ui::scroll("detail_sidebar_scroll", SpacingScale::Xs, sidebar_children),
                    reader_pane,
                ],
            )
        };
        top_nodes.push(body_node);

        let padding = if self.compact {
            SpacingScale::Sm
        } else {
            SpacingScale::Md
        };
        let root = ui::scroll(
            "library_detail_scroll",
            padding,
            vec![ui::vstack(
                "library_detail_root",
                SpacingScale::Md,
                SpacingScale::None,
                top_nodes,
            )],
        );
        ui::ui_tree(self.revision, root)
    }
}

impl SlotPlugin for LibraryPlugin {
    fn render(
        &mut self,
        ctx: &SlotRenderContext,
        host: &dyn HostTransport,
    ) -> Result<UiTree, String> {
        self.compact = ctx.compact;
        match self.view {
            LibraryView::List => {
                let error = self.load_page(host);
                Ok(self.build_list_tree(error.as_deref()))
            }
            LibraryView::Detail(_) => {
                let error = self.refresh_current_detail(host);
                Ok(self.build_detail_tree(error.as_deref()))
            }
        }
    }

    fn handle_event(
        &mut self,
        event: &UiEventEnvelope,
        host: &dyn HostTransport,
    ) -> Result<SlotUpdate, String> {
        let action_id = event.action_id.as_str();

        if action_id == "back_to_list" {
            self.view = LibraryView::List;
            self.status_notice = None;
            let error = self.load_page(host);
            return Ok(SlotUpdate::Tree(self.build_list_tree(error.as_deref())));
        }

        if action_id == "refresh_docs" {
            self.view = LibraryView::List;
            self.status_notice = None;
            let error = self.load_page(host);
            return Ok(SlotUpdate::Tree(self.build_list_tree(error.as_deref())));
        }

        if action_id == "host_import_pdf" || action_id == "host_import_epub" {
            self.view = LibraryView::List;
            self.current_cursor = None;
            self.prev_cursors.clear();
            let format_name = if action_id == "host_import_pdf" {
                "PDF"
            } else {
                "EPUB"
            };
            self.status_notice = Some(format!(
                "已请求宿主导入 {format_name}，并同步刷新资料列表。"
            ));
            let error = self.load_page(host);
            return Ok(SlotUpdate::Tree(self.build_list_tree(error.as_deref())));
        }

        if action_id == "filter_format" {
            let UiAction::TabChanged { ref tab_id } = event.action else {
                return Err("filter_format expects TabChanged action".to_owned());
            };
            let filter = FormatFilter::from_tab_id(tab_id)
                .ok_or_else(|| format!("unknown format tab `{tab_id}`"))?;
            self.filter = filter;
            self.current_cursor = None;
            self.prev_cursors.clear();
            self.status_notice = None;
            self.view = LibraryView::List;
            let error = self.load_page(host);
            return Ok(SlotUpdate::Tree(self.build_list_tree(error.as_deref())));
        }

        if action_id == "search_changed" || action_id == "search_submit" {
            match &event.action {
                UiAction::TextChanged { value } | UiAction::TextSubmitted { value } => {
                    self.search_query = value.clone();
                }
                _ => {
                    return Err("search input expects TextChanged or TextSubmitted".to_owned());
                }
            }
            self.status_notice = None;
            return Ok(SlotUpdate::Tree(self.build_list_tree(None)));
        }

        if action_id == "next_page" {
            if let Some(next) = self.next_cursor.clone() {
                self.prev_cursors.push(self.current_cursor.clone());
                self.current_cursor = Some(next);
            }
            self.status_notice = None;
            let error = self.load_page(host);
            return Ok(SlotUpdate::Tree(self.build_list_tree(error.as_deref())));
        }

        if action_id == "prev_page" {
            if let Some(prev) = self.prev_cursors.pop() {
                self.current_cursor = prev;
            }
            self.status_notice = None;
            let error = self.load_page(host);
            return Ok(SlotUpdate::Tree(self.build_list_tree(error.as_deref())));
        }

        if let Some(rest) = action_id.strip_prefix("open_doc:") {
            let (format, doc_id) = parse_open_doc(rest)?;
            let error = self.open_document(format, doc_id, host);
            return Ok(match self.view {
                LibraryView::Detail(_) => {
                    SlotUpdate::Tree(self.build_detail_tree(error.as_deref()))
                }
                LibraryView::List => SlotUpdate::Tree(self.build_list_tree(error.as_deref())),
            });
        }

        if let Some(raw_idx) = action_id.strip_prefix("select_section:") {
            let idx = raw_idx
                .parse::<u32>()
                .map_err(|_| format!("invalid section index `{raw_idx}`"))?;
            if let LibraryView::Detail(ref mut detail) = self.view {
                let max_idx = detail.doc.section_count.saturating_sub(1);
                detail.section_index = idx.min(max_idx);
                detail.confirming_delete = false;
            }
            let error = self.refresh_current_detail(host);
            return Ok(SlotUpdate::Tree(self.build_detail_tree(error.as_deref())));
        }

        let error = match action_id {
            "refresh_doc" => self.refresh_current_detail(host),
            "prev_section" => {
                if let LibraryView::Detail(ref mut detail) = self.view {
                    detail.section_index = detail.section_index.saturating_sub(1);
                    detail.confirming_delete = false;
                }
                self.refresh_current_detail(host)
            }
            "next_section" => {
                if let LibraryView::Detail(ref mut detail) = self.view {
                    let max_idx = detail.doc.section_count.saturating_sub(1);
                    if detail.section_index < max_idx {
                        detail.section_index += 1;
                    }
                    detail.confirming_delete = false;
                }
                self.refresh_current_detail(host)
            }
            "request_delete" => {
                if let LibraryView::Detail(ref mut detail) = self.view {
                    detail.confirming_delete = true;
                }
                None
            }
            "cancel_delete" => {
                if let LibraryView::Detail(ref mut detail) = self.view {
                    detail.confirming_delete = false;
                }
                None
            }
            "confirm_delete" => self.delete_active_document(host),
            other => Some(format!("未知操作: {other}")),
        };

        let tree = match self.view {
            LibraryView::List => self.build_list_tree(error.as_deref()),
            LibraryView::Detail(_) => self.build_detail_tree(error.as_deref()),
        };
        Ok(SlotUpdate::Tree(tree))
    }
}

fn parse_open_doc(rest: &str) -> Result<(DocumentFormat, i64), String> {
    let (fmt, raw_id) = rest
        .split_once(':')
        .ok_or_else(|| format!("invalid open_doc target `{rest}`"))?;
    let format = match fmt {
        "pdf" => DocumentFormat::Pdf,
        "epub" => DocumentFormat::Epub,
        _ => return Err(format!("unknown document format `{fmt}`")),
    };
    let doc_id = raw_id
        .parse::<i64>()
        .map_err(|_| format!("invalid document id `{raw_id}`"))?;
    Ok((format, doc_id))
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

fn filter_label(filter: FormatFilter) -> &'static str {
    match filter {
        FormatFilter::All => "全部",
        FormatFilter::Pdf => "PDF",
        FormatFilter::Epub => "EPUB",
    }
}

fn section_unit(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Pdf => "页",
        DocumentFormat::Epub => "章",
    }
}

fn singleline(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "未命名".to_owned()
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

fn info_card(id: &str, message: &str) -> UiNode {
    ui::card(
        id,
        SpacingScale::Xs,
        SpacingScale::Sm,
        vec![
            ui::badge(
                format!("{id}_badge"),
                "资料库提示",
                SemanticToken::SurfaceAccent,
            ),
            ui::text(
                format!("{id}_text"),
                message.to_owned(),
                TypographyRole::Caption,
                SemanticToken::TextSecondary,
            ),
        ],
    )
}
