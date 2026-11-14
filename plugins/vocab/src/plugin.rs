#![forbid(unsafe_code)]

use qusor_sdk::ui::{
    Align, SemanticToken, SpacingScale, TypographyRole, UiComponent, UiEventEnvelope, UiNode,
    UiTree,
};
use qusor_sdk::ui_builder as ui;
use qusor_sdk::vocab::{
    MAX_VOCAB_PAGE, VocabCardView, VocabDeck, VocabRating, VocabReviewOutcome, VocabStage,
};
use qusor_sdk::{HostTransport, SlotPlugin, SlotRenderContext, SlotUpdate, VocabClient};

#[derive(Default)]
pub struct VocabPlugin {
    revision: u64,
    request_seq: u64,
    pending_review_op: Option<(i64, String)>,
    compact: bool,
    view: VocabView,
}

#[derive(Default)]
enum VocabView {
    #[default]
    Decks,
    Studying(StudyState),
}

struct StudyState {
    wordbook_id: i64,
    title: String,
    queue: Vec<VocabCardView>,
    remaining_due: u32,
    remaining_new: u32,
    revealed: bool,
    last_outcome: Option<VocabReviewOutcome>,
    reviewed_in_round: u32,
}

impl VocabPlugin {
    fn next_req_id(&mut self, prefix: &str) -> String {
        self.request_seq += 1;
        format!("{prefix}-{}", self.request_seq)
    }

    fn review_op_id(&mut self, card_id: i64) -> String {
        if let Some((pending_card_id, ref existing)) = self.pending_review_op
            && pending_card_id == card_id
        {
            return existing.clone();
        }
        let op_id = self.next_req_id(&format!("vocab-rev-{card_id}"));
        self.pending_review_op = Some((card_id, op_id.clone()));
        op_id
    }

    fn load_decks(&mut self, host: &dyn HostTransport) -> (Vec<VocabDeck>, Option<String>) {
        let req_id = self.next_req_id("vocab-decks");
        let client = VocabClient::with_transport(host);
        match client.decks(&req_id) {
            Ok(decks) => (decks, None),
            Err(err) => (Vec::new(), Some(err.to_string())),
        }
    }

    fn open_deck(
        &mut self,
        wordbook_id: i64,
        decks: &[VocabDeck],
        host: &dyn HostTransport,
    ) -> Option<String> {
        let title = decks
            .iter()
            .find(|deck| deck.wordbook_id == wordbook_id)
            .map(|deck| deck.title.clone())
            .unwrap_or_else(|| format!("词书 #{wordbook_id}"));
        let req_id = self.next_req_id("vocab-queue");
        let client = VocabClient::with_transport(host);
        match client.queue(&req_id, wordbook_id, MAX_VOCAB_PAGE) {
            Ok(page) => {
                self.pending_review_op = None;
                self.view = VocabView::Studying(StudyState {
                    wordbook_id,
                    title,
                    queue: page.cards,
                    remaining_due: page.remaining_due,
                    remaining_new: page.remaining_new,
                    revealed: false,
                    last_outcome: None,
                    reviewed_in_round: 0,
                });
                None
            }
            Err(err) => Some(err.to_string()),
        }
    }

    fn refresh_active_queue(&mut self, host: &dyn HostTransport) -> Option<String> {
        let wordbook_id = match &self.view {
            VocabView::Studying(state) => state.wordbook_id,
            VocabView::Decks => return None,
        };
        let req_id = self.next_req_id("vocab-queue");
        let client = VocabClient::with_transport(host);
        match client.queue(&req_id, wordbook_id, MAX_VOCAB_PAGE) {
            Ok(page) => {
                if let VocabView::Studying(state) = &mut self.view {
                    state.queue = page.cards;
                    state.remaining_due = page.remaining_due;
                    state.remaining_new = page.remaining_new;
                    state.revealed = false;
                }
                self.pending_review_op = None;
                None
            }
            Err(err) => Some(err.to_string()),
        }
    }

    fn submit_rating(&mut self, rating: VocabRating, host: &dyn HostTransport) -> Option<String> {
        let card_id = match &self.view {
            VocabView::Studying(state) => state.queue.first().map(|card| card.card_id),
            VocabView::Decks => None,
        }?;
        let op_id = self.review_op_id(card_id);
        let outcome = {
            let client = VocabClient::with_transport(host);
            client.review(&op_id, card_id, rating)
        };
        let outcome = match outcome {
            Ok(ok) => ok,
            Err(err) => return Some(err.to_string()),
        };

        self.pending_review_op = None;
        let need_more = match &mut self.view {
            VocabView::Studying(state) => {
                if !state.queue.is_empty() {
                    state.queue.remove(0);
                }
                state.revealed = false;
                state.reviewed_in_round += 1;
                state.remaining_due = outcome.remaining_due;
                state.remaining_new = outcome.remaining_new;
                state.last_outcome = Some(outcome);
                state.queue.is_empty() && (state.remaining_due > 0 || state.remaining_new > 0)
            }
            VocabView::Decks => false,
        };

        if need_more {
            return self.refresh_active_queue(host);
        }
        None
    }

    fn build_decks_tree(&mut self, decks: &[VocabDeck], error: Option<&str>) -> UiTree {
        self.revision = self.revision.saturating_add(1).max(1);
        let mut children = vec![ui::hstack(
            "vocab_header",
            SpacingScale::Sm,
            Align::Center,
            vec![
                ui::text(
                    "vocab_title",
                    "背单词",
                    TypographyRole::Title,
                    SemanticToken::TextPrimary,
                ),
                ui::badge("vocab_badge", "Wasm 沙盒插件", SemanticToken::SurfaceAccent),
                ui::button(
                    "vocab_refresh_decks",
                    "刷新词书",
                    "refresh_decks",
                    false,
                    false,
                    false,
                ),
            ],
        )];

        if let Some(message) = error {
            children.push(error_card("vocab_decks_error", message));
        }

        if decks.is_empty() {
            children.push(ui::empty_state(
                "vocab_decks_empty",
                "暂无词书",
                "请先在知识库中导入词书或生词条目。",
            ));
        } else {
            let items: Vec<UiNode> = decks
                .iter()
                .map(|deck| {
                    ui::list_item(
                        format!("deck_item_{}", deck.wordbook_id),
                        deck.title.clone(),
                        Some(format!(
                            "待复习 {} · 新词 {} · 共 {}",
                            deck.due, deck.new, deck.total
                        )),
                        false,
                        format!("open_deck:{}", deck.wordbook_id),
                    )
                })
                .collect();
            children.push(ui::section(
                "vocab_decks_section",
                "选择词书开始学习",
                SpacingScale::Sm,
                vec![ui::list("vocab_deck_list", SpacingScale::Xs, items)],
            ));
        }

        let root = ui::scroll(
            "vocab_decks_scroll",
            SpacingScale::Md,
            vec![ui::vstack(
                "vocab_decks_root",
                SpacingScale::Md,
                SpacingScale::None,
                children,
            )],
        );
        ui::ui_tree(self.revision, root)
    }

    fn build_study_tree(&mut self, error: Option<&str>) -> UiTree {
        self.revision = self.revision.saturating_add(1).max(1);
        let VocabView::Studying(ref state) = self.view else {
            return ui::ui_tree(
                self.revision,
                ui::empty_state("vocab_invalid_state", "状态异常", "请返回词书列表重试。"),
            );
        };

        let pending = state.remaining_due.saturating_add(state.remaining_new);
        let progress_total = state.reviewed_in_round.saturating_add(pending).max(1);
        let progress_done = state.reviewed_in_round.min(progress_total);

        let mut children = vec![
            ui::hstack(
                "study_toolbar",
                SpacingScale::Sm,
                Align::Center,
                vec![
                    ui::button(
                        "study_back_btn",
                        "返回词书",
                        "back_to_decks",
                        false,
                        false,
                        false,
                    ),
                    ui::text(
                        "study_deck_title",
                        state.title.clone(),
                        TypographyRole::Subtitle,
                        SemanticToken::TextPrimary,
                    ),
                    ui::badge(
                        "study_counter_badge",
                        format!(
                            "待复习 {} · 新词 {}",
                            state.remaining_due, state.remaining_new
                        ),
                        SemanticToken::SurfaceInset,
                    ),
                    ui::button(
                        "study_refresh_btn",
                        "刷新队列",
                        "refresh_queue",
                        false,
                        false,
                        false,
                    ),
                ],
            ),
            ui::progress("study_progress", progress_done, progress_total),
        ];

        if let Some(message) = error {
            children.push(error_card("study_error_card", message));
        }

        if let Some(ref outcome) = state.last_outcome {
            children.push(ui::key_value(
                "study_last_outcome",
                "上次调度",
                format!(
                    "{} · {} 分钟后到期",
                    stage_label(outcome.stage),
                    outcome.scheduled_minutes
                ),
            ));
        }

        if let Some(card) = state.queue.first() {
            let mut card_nodes = vec![
                ui::hstack(
                    "card_head",
                    SpacingScale::Sm,
                    Align::Center,
                    vec![
                        ui::text(
                            "card_term",
                            card.term.clone(),
                            TypographyRole::Title,
                            SemanticToken::TextPrimary,
                        ),
                        ui::badge(
                            "card_stage",
                            stage_label(card.stage),
                            SemanticToken::SurfaceAccent,
                        ),
                    ],
                ),
                ui::node("card_divider", UiComponent::Divider, Vec::new()),
            ];

            if state.revealed {
                card_nodes.push(ui::key_value("card_meaning", "释义", card.meaning.clone()));
                if !card.example.trim().is_empty() {
                    card_nodes.push(ui::text(
                        "card_example",
                        format!("例句：{}", card.example),
                        TypographyRole::Body,
                        SemanticToken::TextSecondary,
                    ));
                }
                card_nodes.push(ui::hstack(
                    "card_rating_row",
                    SpacingScale::Sm,
                    Align::Center,
                    vec![
                        ui::button(
                            "rate_again_btn",
                            "Again 重来",
                            "rate_again",
                            false,
                            true,
                            false,
                        ),
                        ui::button(
                            "rate_hard_btn",
                            "Hard 困难",
                            "rate_hard",
                            false,
                            false,
                            false,
                        ),
                        ui::button(
                            "rate_good_btn",
                            "Good 认识",
                            "rate_good",
                            true,
                            false,
                            false,
                        ),
                        ui::button(
                            "rate_easy_btn",
                            "Easy 掌握",
                            "rate_easy",
                            false,
                            false,
                            false,
                        ),
                    ],
                ));
            } else {
                card_nodes.push(ui::text(
                    "card_hint",
                    "思考词义与语境后，点击下方按钮揭晓释义与例句。",
                    TypographyRole::Caption,
                    SemanticToken::MutedForeground,
                ));
                card_nodes.push(ui::button(
                    "card_reveal_btn",
                    "揭晓释义",
                    "reveal_card",
                    true,
                    false,
                    false,
                ));
            }

            children.push(ui::card(
                "study_active_card",
                SpacingScale::Md,
                SpacingScale::Lg,
                card_nodes,
            ));
        } else {
            children.push(ui::empty_state(
                "study_round_done",
                "本轮学习已完成",
                format!(
                    "「{}」当前没有更多到期复习或新词（本轮已提交 {} 次复习）。",
                    state.title, state.reviewed_in_round
                ),
            ));
            children.push(ui::hstack(
                "study_done_actions",
                SpacingScale::Sm,
                Align::Center,
                vec![
                    ui::button(
                        "study_done_back_btn",
                        "返回词书列表",
                        "back_to_decks",
                        true,
                        false,
                        false,
                    ),
                    ui::button(
                        "study_done_recheck_btn",
                        "重新检查队列",
                        "refresh_queue",
                        false,
                        false,
                        false,
                    ),
                ],
            ));
        }

        let padding = if self.compact {
            SpacingScale::Sm
        } else {
            SpacingScale::Md
        };
        let root = ui::scroll(
            "vocab_study_scroll",
            padding,
            vec![ui::vstack(
                "vocab_study_root",
                SpacingScale::Md,
                SpacingScale::None,
                children,
            )],
        );
        ui::ui_tree(self.revision, root)
    }
}

impl SlotPlugin for VocabPlugin {
    fn render(
        &mut self,
        ctx: &SlotRenderContext,
        host: &dyn HostTransport,
    ) -> Result<UiTree, String> {
        self.compact = ctx.compact;
        match self.view {
            VocabView::Decks => {
                let (decks, error) = self.load_decks(host);
                Ok(self.build_decks_tree(&decks, error.as_deref()))
            }
            VocabView::Studying(_) => {
                let error = self.refresh_active_queue(host);
                Ok(self.build_study_tree(error.as_deref()))
            }
        }
    }

    fn handle_event(
        &mut self,
        event: &UiEventEnvelope,
        host: &dyn HostTransport,
    ) -> Result<SlotUpdate, String> {
        let action_id = event.action_id.as_str();
        if action_id == "refresh_decks" || action_id == "back_to_decks" {
            self.pending_review_op = None;
            self.view = VocabView::Decks;
            let (decks, error) = self.load_decks(host);
            return Ok(SlotUpdate::Tree(
                self.build_decks_tree(&decks, error.as_deref()),
            ));
        }

        if let Some(raw_id) = action_id.strip_prefix("open_deck:") {
            let wordbook_id = raw_id
                .parse::<i64>()
                .map_err(|_| format!("invalid deck id `{raw_id}`"))?;
            let (decks, decks_err) = self.load_decks(host);
            if let Some(err) = decks_err {
                return Ok(SlotUpdate::Tree(self.build_decks_tree(&decks, Some(&err))));
            }
            if let Some(err) = self.open_deck(wordbook_id, &decks, host) {
                return Ok(SlotUpdate::Tree(self.build_decks_tree(&decks, Some(&err))));
            }
            return Ok(SlotUpdate::Tree(self.build_study_tree(None)));
        }

        let error = match action_id {
            "refresh_queue" => self.refresh_active_queue(host),
            "reveal_card" => {
                if let VocabView::Studying(ref mut state) = self.view {
                    state.revealed = true;
                }
                None
            }
            "rate_again" => self.submit_rating(VocabRating::Again, host),
            "rate_hard" => self.submit_rating(VocabRating::Hard, host),
            "rate_good" => self.submit_rating(VocabRating::Good, host),
            "rate_easy" => self.submit_rating(VocabRating::Easy, host),
            other => Some(format!("未知操作: {other}")),
        };

        let tree = match self.view {
            VocabView::Decks => {
                let (decks, decks_err) = self.load_decks(host);
                let combined = error.or(decks_err);
                self.build_decks_tree(&decks, combined.as_deref())
            }
            VocabView::Studying(_) => self.build_study_tree(error.as_deref()),
        };
        Ok(SlotUpdate::Tree(tree))
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

fn stage_label(stage: VocabStage) -> &'static str {
    match stage {
        VocabStage::New => "新词",
        VocabStage::Learning => "学习中",
        VocabStage::Review => "复习",
    }
}
