#![cfg_attr(not(target_arch = "wasm32"), forbid(unsafe_code))]
#![deny(unsafe_code)]

pub mod plugin;

qusor_sdk::export_slot_plugin!(crate::plugin::VocabPlugin);

#[cfg(test)]
mod tests {
    use crate::plugin::VocabPlugin;
    use qusor_sdk::ui::{
        PLUGIN_UI_PROTOCOL, UiAction, UiBudgets, UiComponent, UiEventEnvelope, UiNode,
    };
    use qusor_sdk::vocab::{
        VocabCardView, VocabDeck, VocabOperation, VocabRating, VocabRequest, VocabResponse,
        VocabResult, VocabReviewOutcome, VocabStage,
    };
    use qusor_sdk::{
        HOST_API_V0_7, HostTransport, SdkError, SlotPlugin, SlotRenderContext, SlotUpdate,
    };
    use std::cell::RefCell;

    struct FakeHost {
        decks: RefCell<Vec<VocabDeck>>,
        cards: RefCell<Vec<VocabCardView>>,
        reviewed: RefCell<Vec<(String, i64, VocabRating)>>,
        fail_next_review: RefCell<bool>,
    }

    impl FakeHost {
        fn new() -> Self {
            Self {
                decks: RefCell::new(vec![VocabDeck {
                    wordbook_id: 1,
                    title: "四级核心词".into(),
                    total: 2,
                    due: 1,
                    new: 1,
                }]),
                cards: RefCell::new(vec![
                    VocabCardView {
                        card_id: 11,
                        term: "sandbox".into(),
                        meaning: "沙盒".into(),
                        example: "Plugins run inside a sandbox.".into(),
                        stage: VocabStage::Review,
                    },
                    VocabCardView {
                        card_id: 12,
                        term: "contract".into(),
                        meaning: "契约".into(),
                        example: "Versioned contracts keep boundaries strict.".into(),
                        stage: VocabStage::New,
                    },
                ]),
                reviewed: RefCell::new(Vec::new()),
                fail_next_review: RefCell::new(false),
            }
        }
    }

    impl HostTransport for FakeHost {
        fn call_bytes(&self, request: &[u8]) -> Result<Vec<u8>, SdkError> {
            let req = VocabRequest::from_slice(request)
                .map_err(|err| SdkError::Serialize(err.to_string()))?;
            let result = match req.operation {
                VocabOperation::Decks {} => VocabResult::Decks {
                    decks: self.decks.borrow().clone(),
                },
                VocabOperation::Queue { wordbook_id, limit } => {
                    assert_eq!(wordbook_id, 1);
                    let cards: Vec<VocabCardView> = self
                        .cards
                        .borrow()
                        .iter()
                        .take(limit as usize)
                        .cloned()
                        .collect();
                    let due = cards.iter().filter(|c| c.stage != VocabStage::New).count() as u32;
                    let new = cards.iter().filter(|c| c.stage == VocabStage::New).count() as u32;
                    VocabResult::Queue {
                        cards,
                        remaining_due: due,
                        remaining_new: new,
                    }
                }
                VocabOperation::Review { card_id, rating } => {
                    if *self.fail_next_review.borrow() {
                        *self.fail_next_review.borrow_mut() = false;
                        return Err(SdkError::Transport("transient host error".into()));
                    }
                    self.reviewed
                        .borrow_mut()
                        .push((req.request_id.clone(), card_id, rating));
                    self.cards.borrow_mut().retain(|c| c.card_id != card_id);
                    let remaining_due = self
                        .cards
                        .borrow()
                        .iter()
                        .filter(|c| c.stage != VocabStage::New)
                        .count() as u32;
                    let remaining_new = self
                        .cards
                        .borrow()
                        .iter()
                        .filter(|c| c.stage == VocabStage::New)
                        .count() as u32;
                    VocabResult::Review(VocabReviewOutcome {
                        card_id,
                        stage: VocabStage::Review,
                        due_at_ms: 1_767_226_200_000,
                        scheduled_minutes: 10,
                        remaining_due,
                        remaining_new,
                    })
                }
            };
            let resp = VocabResponse {
                request_id: req.request_id,
                result,
            };
            Ok(serde_json::to_vec(&resp).unwrap())
        }
    }

    fn event(node_id: &str, action_id: &str) -> UiEventEnvelope {
        UiEventEnvelope {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: "slot-1".into(),
            generation: 1,
            node_id: node_id.into(),
            action_id: action_id.into(),
            action: UiAction::Press,
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

    #[test]
    fn vocab_plugin_full_flow_decks_reveal_retry_and_complete() {
        let host = FakeHost::new();
        let mut plugin = VocabPlugin::default();
        let initial = plugin
            .render(
                &SlotRenderContext {
                    contribution_id: "org.qusor.vocab.workspace".into(),
                    mount_id: "slot-1".into(),
                    generation: 1,
                    api_version: Some(HOST_API_V0_7),
                    compact: false,
                },
                &host,
            )
            .unwrap();
        initial.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&initial.root, "四级核心词"));

        // Select deck 1
        let SlotUpdate::Tree(study_tree) = plugin
            .handle_event(&event("deck_item_1", "open_deck:1"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        study_tree.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&study_tree.root, "sandbox"));
        assert!(!find_text(&study_tree.root, "沙盒"));

        // Reveal card
        let SlotUpdate::Tree(revealed_tree) = plugin
            .handle_event(&event("card_reveal_btn", "reveal_card"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        revealed_tree.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&revealed_tree.root, "沙盒"));
        assert!(find_text(
            &revealed_tree.root,
            "Plugins run inside a sandbox."
        ));

        // Transient failure keeps same card + allows retry with idempotent op_id
        *host.fail_next_review.borrow_mut() = true;
        let SlotUpdate::Tree(err_tree) = plugin
            .handle_event(&event("rate_good_btn", "rate_good"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        err_tree.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&err_tree.root, "transient host error"));
        assert!(find_text(&err_tree.root, "sandbox"));

        // Retry good rating succeeds and advances to next card
        let SlotUpdate::Tree(next_tree) = plugin
            .handle_event(&event("rate_good_btn", "rate_good"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        next_tree.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&next_tree.root, "contract"));

        // Reveal and rate second card easy -> finishes round
        let _ = plugin
            .handle_event(&event("card_reveal_btn", "reveal_card"), &host)
            .unwrap();
        let SlotUpdate::Tree(done_tree) = plugin
            .handle_event(&event("rate_easy_btn", "rate_easy"), &host)
            .unwrap()
        else {
            panic!("expected tree");
        };
        done_tree.validate(UiBudgets::default()).unwrap();
        assert!(find_text(&done_tree.root, "本轮学习已完成"));
        assert_eq!(host.reviewed.borrow().len(), 2);
        assert_ne!(host.reviewed.borrow()[0].0, host.reviewed.borrow()[1].0);
    }
}
