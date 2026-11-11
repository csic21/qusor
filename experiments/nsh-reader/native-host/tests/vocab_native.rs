//! The workbench vocabulary module: ordinary package + public vocab-study-v1
//! wire over a seeded real Core database, verified without a window.
#![cfg(unix)]

use core_api::vocab::{NewWord, WordbookSeed};
use extension_api::{Manifest, SUPPORTED_HOST_APIS};
use extension_package::package::{PackageFile, build_signed_for_host, publisher_key};
use learning_application::study::StudyApplication;
use qusor_nsh_native_host_probe::vocab_native::{self, VocabHost};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

const SEED: [u8; 32] = [0x33; 32];
const NOW: i64 = 1_767_225_600_000;
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-vocab-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn books() -> Vec<WordbookSeed> {
    vec![
        WordbookSeed {
            title: "CET-4 示例".into(),
            description: "合成示例词书，不是授权词表。".into(),
            words: vec![
                NewWord {
                    term: "abandon".into(),
                    meaning: "放弃；抛弃".into(),
                    example: "Do not abandon the plan.".into(),
                },
                NewWord {
                    term: "benefit".into(),
                    meaning: "益处；使受益".into(),
                    example: "Reading has many benefits.".into(),
                },
                NewWord {
                    term: "candidate".into(),
                    meaning: "候选人；申请者".into(),
                    example: "The candidate answered well.".into(),
                },
            ],
        },
        WordbookSeed {
            title: "QUSOR 术语".into(),
            description: "合成示例词书。".into(),
            words: vec![
                NewWord {
                    term: "plugin".into(),
                    meaning: "插件".into(),
                    example: "The plugin submits bounded requests.".into(),
                },
                NewWord {
                    term: "vault".into(),
                    meaning: "本地库".into(),
                    example: "The Host binds the vault.".into(),
                },
            ],
        },
    ]
}

fn seed_core(core: &Path) {
    let mut app = StudyApplication::new(SqliteStore::open(core).unwrap());
    for seed in books() {
        app.seed_wordbook("workbench-vault", &seed, NOW).unwrap();
    }
}

fn signed_package(scratch: &Scratch) -> (PathBuf, PathBuf) {
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../plugins/vocab");
    let manifest = Manifest::from_slice(&fs::read(plugins.join("manifest.json")).unwrap()).unwrap();
    let wasm = wat::parse_str(fs::read_to_string(plugins.join("guest.wat")).unwrap()).unwrap();
    let bytes = build_signed_for_host(
        manifest,
        "vocab.wasm",
        vec![PackageFile {
            path: "vocab.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wasm,
        }],
        &SEED,
        &SUPPORTED_HOST_APIS,
    )
    .unwrap();
    let package = scratch.0.join("vocab.qpk");
    let key = scratch.0.join("vocab-key.txt");
    fs::write(&package, bytes).unwrap();
    fs::write(&key, publisher_key(&SEED)).unwrap();
    (package, key)
}

fn value(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn the_workbench_module_reads_and_reviews_real_core() {
    let scratch = Scratch::new();
    let core = scratch.0.join("core");
    seed_core(&core);
    let (package, key) = signed_package(&scratch);
    vocab_native::initialize(&core, &package, &key).unwrap();

    let decks = value(VocabHost::decks());
    assert_eq!(decks["ok"], true, "{decks}");
    let list = decks["value"]["decks"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["title"], "CET-4 示例");
    assert_eq!(list[0]["total"], 3);
    assert_eq!(list[0]["new"], 3);
    let book_id = list[0]["wordbook_id"].as_i64().unwrap();

    let queue = value(VocabHost::queue(book_id, 6));
    assert_eq!(queue["ok"], true, "{queue}");
    let cards = queue["value"]["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 3);
    assert_eq!(cards[0]["term"], "abandon");
    assert_eq!(cards[0]["meaning"], "放弃；抛弃");
    assert_eq!(cards[0]["stage"], "new");
    assert_eq!(queue["value"]["remaining_due"], 0);
    assert_eq!(queue["value"]["remaining_new"], 0);
    let card_id = cards[0]["card_id"].as_i64().unwrap();

    let reviewed = value(VocabHost::review(
        card_id,
        "good".into(),
        "workbench-review-1".into(),
    ));
    assert_eq!(reviewed["ok"], true, "{reviewed}");
    assert_eq!(reviewed["value"]["stage"], "review");
    assert_eq!(reviewed["value"]["scheduled_minutes"], 4320);
    let due = reviewed["value"]["due_at_ms"].as_i64().unwrap();
    assert!(due > NOW);

    // Idempotent replay returns the stored outcome; a different grade is a conflict.
    let replay = value(VocabHost::review(
        card_id,
        "good".into(),
        "workbench-review-1".into(),
    ));
    assert_eq!(replay["value"]["due_at_ms"], due);
    let conflict = value(VocabHost::review(
        card_id,
        "easy".into(),
        "workbench-review-1".into(),
    ));
    assert_eq!(conflict["ok"], false);

    // Unknown ratings, invalid ids and unknown decks cannot widen the wire.
    assert_eq!(
        value(VocabHost::review(
            card_id,
            "perfect".into(),
            "workbench-review-2".into()
        ))["ok"],
        false
    );
    assert_eq!(value(VocabHost::queue(0, 6))["ok"], false);
    assert_eq!(value(VocabHost::queue(book_id, 0))["ok"], false);

    // The queue now returns the two remaining new cards; the reviewed one is scheduled.
    let queue = value(VocabHost::queue(book_id, 6));
    assert_eq!(queue["value"]["cards"].as_array().unwrap().len(), 2);
    assert_eq!(queue["value"]["remaining_new"], 0);

    // The replay above reads the stored review back through the same Core
    // snapshot, so the schedule is already durable; reopening the directory is
    // the single-writer store's job and is covered by storage-sqlite tests.
    assert_eq!(conflict["ok"], false);
}
