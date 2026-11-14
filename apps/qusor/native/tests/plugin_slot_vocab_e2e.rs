#![cfg(unix)]
//! Real end-to-end integration test driving the compiled `vocab.qpk` Rust Wasm
//! plugin through `PluginSlotHost` -> `WasmiSession` -> `Host::authorize_*` ->
//! `VocabBridge` -> real SQLite Core -> `PluginUiHost` generation fencing.

use extension_api::ui::{PLUGIN_UI_PROTOCOL, UiAction, UiBudgets, UiEventEnvelope, UiTree};
use nsh_core::prelude::System;
use qusor_app_native::{
    app_info::AppInfo, composition_native::CompositionHost, plugin_slot::PluginSlotHost,
    vocab_native::VocabHost,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-plugin-slot-e2e-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn parse_json(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap()
}

fn press_envelope(mount_id: &str, generation: u64, node_id: &str, action_id: &str) -> String {
    serde_json::to_string(&UiEventEnvelope {
        protocol_version: PLUGIN_UI_PROTOCOL,
        mount_id: mount_id.to_owned(),
        generation,
        node_id: node_id.to_owned(),
        action_id: action_id.to_owned(),
        action: UiAction::Press,
    })
    .unwrap()
}

#[test]
fn real_vocab_wasm_plugin_drives_study_round_and_persists_to_sqlite() {
    let scratch = Scratch::new();
    let mut system = System::default();
    system.data_dir = Some(scratch.0.clone());

    let import = json!({
        "title": "E2E Plugin Deck",
        "description": "Real Wasm + SQLite end-to-end verification deck",
        "words": [
            {
                "term": "serendipity",
                "pos": "n.",
                "phonetic": "/ˌserənˈdɪpəti/",
                "meaning": "意外收获",
                "example": "Finding this plugin architecture working end-to-end was pure serendipity."
            },
            {
                "term": "ephemeral",
                "pos": "adj.",
                "phonetic": "/ɪˈfemərəl/",
                "meaning": "短暂的",
                "example": "Wasm memory resets across remounts while Core state persists."
            }
        ]
    })
    .to_string();
    let imported = parse_json(VocabHost::import_wordbook(system.clone(), import));
    assert_eq!(imported["ok"], true, "{imported}");
    let wordbook_id = imported["value"]["wordbook_id"].as_i64().unwrap();

    // 1. Mount real embedded `vocab.qpk` via generic `PluginSlotHost`.
    let mounted = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.vocab.workspace".into(),
    ));
    assert_eq!(mounted["ok"], true, "{mounted}");
    let mount_id = mounted["value"]["mount_id"].as_str().unwrap().to_owned();
    let generation = mounted["value"]["generation"].as_u64().unwrap();
    let initial_tree_json = serde_json::to_vec(&mounted["value"]["tree"]).unwrap();
    let initial_tree =
        UiTree::from_slice_with_budgets(&initial_tree_json, UiBudgets::default()).unwrap();
    let initial_text = serde_json::to_string(&initial_tree).unwrap();
    assert!(
        initial_text.contains("E2E Plugin Deck"),
        "expected decks UiTree to contain imported deck: {initial_text}"
    );
    assert!(
        initial_text.contains(&format!("open_deck:{wordbook_id}")),
        "expected open_deck action for wordbook {wordbook_id}"
    );

    // 2. Open the imported deck and inspect first card before reveal.
    let opened = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(
            &mount_id,
            generation,
            &format!("deck_item_{wordbook_id}"),
            &format!("open_deck:{wordbook_id}"),
        ),
    ));
    assert_eq!(opened["ok"], true, "{opened}");
    let opened_text = opened["value"]["tree"].to_string();
    assert!(opened_text.contains("serendipity"), "{opened_text}");
    assert!(opened_text.contains("reveal_card"), "{opened_text}");
    assert!(
        !opened_text.contains("rate_good"),
        "rating actions should stay hidden before reveal: {opened_text}"
    );

    // 3. Reveal card meaning and example.
    let revealed = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(&mount_id, generation, "card_reveal_btn", "reveal_card"),
    ));
    assert_eq!(revealed["ok"], true, "{revealed}");
    let revealed_text = revealed["value"]["tree"].to_string();
    assert!(revealed_text.contains("意外收获"), "{revealed_text}");
    assert!(revealed_text.contains("rate_good"), "{revealed_text}");

    // 4. Submit `good` rating -> advances to second card & persists review in SQLite Core.
    let rated = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(&mount_id, generation, "rate_good_btn", "rate_good"),
    ));
    assert_eq!(rated["ok"], true, "{rated}");
    let rated_tree_bytes = serde_json::to_vec(&rated["value"]["tree"]).unwrap();
    let rated_tree =
        UiTree::from_slice_with_budgets(&rated_tree_bytes, UiBudgets::default()).unwrap();
    let rated_text = serde_json::to_string(&rated_tree).unwrap();
    assert!(rated_text.contains("ephemeral"), "{rated_text}");
    assert!(rated_text.contains("上次调度"), "{rated_text}");

    // 5. Verify SQLite Core persistence via backup export & VocabHost::decks.
    let backup = parse_json(AppInfo::export_backup(system.clone()));
    assert_eq!(backup["ok"], true, "{backup}");
    let backup_path = backup["value"]["path"].as_str().unwrap();
    let backup_doc: Value =
        serde_json::from_str(&fs::read_to_string(backup_path).unwrap()).unwrap();
    let reviews = backup_doc["study"]["reviews"].as_array().unwrap();
    assert_eq!(
        reviews.len(),
        1,
        "expected exactly 1 persisted review record in SQLite study snapshot"
    );
    assert!(
        reviews.iter().any(|review| review["rating"] == "good"),
        "expected persisted review rating 'good', got {reviews:?}"
    );
    let decks = parse_json(VocabHost::decks(system.clone()));
    assert_eq!(decks["ok"], true, "{decks}");
    let deck = decks["value"]["decks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["wordbook_id"] == wordbook_id)
        .expect("imported wordbook missing from decks");
    assert_eq!(deck["total"], 2);
    assert_eq!(deck["new"], 1);

    // 6. Reject stale generation events (generation fencing).
    let stale = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(
            &mount_id,
            generation.saturating_sub(1),
            "card_reveal_btn",
            "reveal_card",
        ),
    ));
    assert_eq!(stale["ok"], false, "{stale}");

    // 7. Disabling the package immediately unmounts its live slot and blocks re-mount.
    let disabled = parse_json(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.vocab".into(),
        false,
    ));
    assert_eq!(disabled["ok"], true, "{disabled}");
    let after_disable = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(&mount_id, generation, "card_reveal_btn", "reveal_card"),
    ));
    assert_eq!(after_disable["ok"], false, "{after_disable}");
    let remount_while_disabled = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.vocab.workspace".into(),
    ));
    assert_eq!(
        remount_while_disabled["ok"], false,
        "{remount_while_disabled}"
    );

    // 8. Re-enabling lets a fresh slot remount and still sees updated Core counts (`待复习 ... 新词 1`).
    let reenabled = parse_json(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.vocab".into(),
        true,
    ));
    assert_eq!(reenabled["ok"], true, "{reenabled}");
    let remounted = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.vocab.workspace".into(),
    ));
    assert_eq!(remounted["ok"], true, "{remounted}");
    let remount_mount_id = remounted["value"]["mount_id"].as_str().unwrap().to_owned();
    let remount_text = remounted["value"]["tree"].to_string();
    assert!(
        remount_text.contains("新词 1 · 共 2"),
        "remounted plugin should read persisted new=1 total=2 from SQLite Core: {remount_text}"
    );

    let unmounted = parse_json(PluginSlotHost::unmount_slot(system, remount_mount_id));
    assert_eq!(unmounted["ok"], true, "{unmounted}");
}
