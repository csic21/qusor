//! App identity and the receipts a self-update evidence run leaves behind.
//!
//! `info` is the production surface: the id, version and build `nsh package`
//! wrote into the app's own `nsh-updater.json` asset, so the shell can name the
//! running version and the update dialog can say what an update replaces.
//!
//! Everything else is evidence plumbing for the local update self-test, and it
//! only works after an evidence run armed it: `QUSOR_UPDATE_EVIDENCE` names the
//! directory in the first run, and the marker that run writes into the app's
//! data directory tells the restarted build where to report. A normal launch has
//! neither, so it cannot write receipts, arm a restart or quit through this
//! module. Core knowledge and package authority never pass through here.

use crate::core_facade;
use core_api::{
    knowledge::{ArchiveV2, validate_archive},
    vocab::{StudyArchive, validate_archive as validate_study_archive},
};
use nsh_core::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// Names the directory receipts go to; set only by the self-test harness.
const EVIDENCE_ENV: &str = "QUSOR_UPDATE_EVIDENCE";
/// The marker an evidence run leaves for the build that replaces it.
const MARKER: &str = "update-evidence.json";
/// A receipt is one short step name and one bounded JSON payload.
const MAX_STEP_BYTES: usize = 48;
const MAX_PAYLOAD_BYTES: usize = 8 * 1024;

/// The packaged app's own description, as `nsh package` wrote it.
struct Config {
    id: String,
    version: String,
    build: Option<u64>,
    /// Where the update source points: the GitHub repository, or the manifest
    /// URL of an own host. `None` when the app has no updater.
    source: Option<Source>,
}

struct Source {
    github: Option<String>,
    url: Option<String>,
}

fn config() -> Option<Config> {
    let executable = std::env::current_exe().ok()?;
    let contents = executable.parent()?.parent()?;
    let asset = contents
        .join("Resources")
        .join("nsh")
        .join("assets")
        .join("nsh-updater.json");
    let value: Value = serde_json::from_slice(&fs::read(asset).ok()?).ok()?;
    let source = Some(Source {
        github: value
            .get("github")
            .and_then(Value::as_str)
            .map(str::to_owned),
        url: value.get("url").and_then(Value::as_str).map(str::to_owned),
    });
    Some(Config {
        id: value.get("id")?.as_str()?.to_owned(),
        version: value.get("version")?.as_str()?.to_owned(),
        build: value.get("build").and_then(Value::as_u64),
        source,
    })
}

fn data_dir(sys: &System) -> Result<PathBuf, String> {
    sys.data_dir
        .clone()
        .ok_or_else(|| "the host gave no app data directory".into())
}

fn marker_path(sys: &System) -> Result<PathBuf, String> {
    Ok(data_dir(sys)?.join(MARKER))
}

/// A marker counts only while it is fresh and names another build: a leftover
/// marker from an interrupted self-test must not turn a later normal launch into
/// an evidence run.
const MARKER_MAX_AGE_MS: u64 = 15 * 60 * 1000;

fn marker_value(sys: &System) -> Option<(PathBuf, Value)> {
    let path = marker_path(sys).ok()?;
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    Some((PathBuf::from(value.get("dir")?.as_str()?), value))
}

fn marker_is_live_at(value: &Value, current: Option<&Config>, now: u64) -> bool {
    let fresh = value
        .get("at_ms")
        .and_then(Value::as_u64)
        .is_some_and(|at| now.saturating_sub(at) <= MARKER_MAX_AGE_MS);
    let another_build = current.is_some_and(|config| {
        value
            .get("from_version")
            .and_then(Value::as_str)
            .is_some_and(|from| from != config.version)
            || value
                .get("from_build")
                .and_then(Value::as_u64)
                .is_some_and(|from| Some(from) != config.build)
    });
    fresh && another_build
}

fn marker_is_live(value: &Value) -> bool {
    marker_is_live_at(value, config().as_ref(), now_ms())
}

/// Where this run reports evidence: `None` when the run is not an evidence run.
fn evidence(sys: &System) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(EVIDENCE_ENV).filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let (dir, value) = marker_value(sys)?;
    (marker_is_live(&value) && dir.is_dir()).then_some(dir)
}

/// Whether a previous evidence build armed this one: it reports the restarted
/// state once and removes the marker through `consume`.
fn restarted(sys: &System) -> bool {
    marker_value(sys).is_some_and(|(dir, value)| marker_is_live(&value) && dir.is_dir())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| u64::try_from(value.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn valid_step(step: &str) -> bool {
    !step.is_empty()
        && step.len() <= MAX_STEP_BYTES
        && step
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn answer(value: Result<Value, String>) -> String {
    match value {
        Ok(value) => json!({"ok": true, "value": value}).to_string(),
        Err(error) => json!({"ok": false, "error": error}).to_string(),
    }
}

/// The running app's identity plus the evidence receipts of a self-update test
/// (TSX: `qusor/pdf-surface`'s `AppInfo`). Only `info` is meant for normal use.
#[module]
pub struct AppInfo;

const MAX_BACKUP_RESTORE_BYTES: usize = 64 * 1024 * 1024;
const BACKUP_FORMAT: &str = "qusor-backup-v1";
const STARTER_WORDBOOK_TITLE: &str = "QUSOR 入门 12 词";

#[derive(Debug, Serialize, Deserialize)]
struct CombinedBackup {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    exported_at_ms: u64,
    knowledge: ArchiveV2,
    study: StudyArchive,
}

enum ParsedBackup {
    Combined {
        knowledge: ArchiveV2,
        study: StudyArchive,
    },
    KnowledgeOnly(ArchiveV2),
}

fn parse_backup_document(document: &str) -> Result<ParsedBackup, String> {
    if document.trim().is_empty() || document.len() > MAX_BACKUP_RESTORE_BYTES {
        return Err("备份文件为空或超出 64 MiB 限制".into());
    }
    let value: Value =
        serde_json::from_str(document).map_err(|_| "备份内容不是合法的 JSON".to_string())?;
    if let Some(format) = value.get("format")
        && format.as_str() != Some(BACKUP_FORMAT)
    {
        return Err("不支持的备份格式版本".into());
    }
    if value.get("knowledge").is_some() || value.get("study").is_some() {
        let combined: CombinedBackup = serde_json::from_value(value)
            .map_err(|_| "组合备份缺少 knowledge/study 字段或结构无效".to_string())?;
        if combined
            .format
            .as_deref()
            .is_some_and(|fmt| fmt != BACKUP_FORMAT)
        {
            return Err("不支持的备份格式版本".into());
        }
        validate_archive(&combined.knowledge).map_err(|e| e.to_string())?;
        validate_study_archive(&combined.study).map_err(|e| e.to_string())?;
        return Ok(ParsedBackup::Combined {
            knowledge: combined.knowledge,
            study: combined.study,
        });
    }
    let knowledge: ArchiveV2 =
        serde_json::from_value(value).map_err(|_| "无法识别的 Core 备份结构".to_string())?;
    validate_archive(&knowledge).map_err(|e| e.to_string())?;
    Ok(ParsedBackup::KnowledgeOnly(knowledge))
}

fn backup_dir(sys: &System) -> Result<PathBuf, String> {
    Ok(data_dir(sys)?.join("backups"))
}

fn export_backup_with_store(
    sys: &System,
    store: crate::vault::VaultStore,
) -> Result<Value, String> {
    let (knowledge, study) = core_facade::export_backup_archives(&store).map_err(|e| e.to_string())?;
    let now = now_ms();
    let payload = CombinedBackup {
        format: Some(BACKUP_FORMAT.into()),
        exported_at_ms: now,
        knowledge,
        study,
    };
    let json_text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    let dir = backup_dir(sys)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("qusor-backup-{now}.json"));
    let mut file = fs::File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(json_text.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    let bytes = json_text.len();
    Ok(json!({
        "path": path.to_string_lossy(),
        "json": json_text,
        "bytes": bytes,
    }))
}

fn restore_backup_with_store(
    store: crate::vault::VaultStore,
    document: &str,
) -> Result<Value, String> {
    let parsed = parse_backup_document(document)?;
    let (current_knowledge, current_study) =
        core_facade::inspect_backup_state(&store).map_err(|e| e.to_string())?;
    if !current_knowledge.commits.is_empty() {
        return Err("restore requires empty Core; existing data was preserved".into());
    }
    match parsed {
        ParsedBackup::KnowledgeOnly(knowledge) => {
            let receipt = core_facade::restore_knowledge_archive(&store, &knowledge)
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "text_captures": receipt.text_captures,
                "pdf_documents": receipt.pdf_documents,
                "pdf_captures": receipt.pdf_captures,
                "epub_documents": receipt.epub_documents,
                "epub_captures": receipt.epub_captures,
                "wordbooks": 0,
            }))
        }
        ParsedBackup::Combined { knowledge, study } => {
            let has_user_study = !current_study.reviews.is_empty()
                || current_study.wordbooks.len() > 1
                || (current_study.wordbooks.len() == 1
                    && current_study.wordbooks[0].title != STARTER_WORDBOOK_TITLE);
            if has_user_study {
                return Err(
                    "restore requires empty Core; existing user wordbooks were preserved".into(),
                );
            }
            let incoming_has_study = !study.wordbooks.is_empty() || !study.cards.is_empty();
            let receipt = core_facade::restore_knowledge_archive(&store, &knowledge)
                .map_err(|e| e.to_string())?;
            if incoming_has_study {
                let replace_starter_wordbook_id = if current_study.wordbooks.len() == 1
                    && current_study.wordbooks[0].title == STARTER_WORDBOOK_TITLE
                    && current_study.reviews.is_empty()
                {
                    Some(current_study.wordbooks[0].id)
                } else {
                    None
                };
                core_facade::restore_study_archive(&store, replace_starter_wordbook_id, &study)
                    .map_err(|e| e.to_string())?;
            }
            Ok(json!({
                "text_captures": receipt.text_captures,
                "pdf_documents": receipt.pdf_documents,
                "pdf_captures": receipt.pdf_captures,
                "epub_documents": receipt.epub_documents,
                "epub_captures": receipt.epub_captures,
                "wordbooks": study.wordbooks.len(),
            }))
        }
    }
}

#[module]
impl AppInfo {
    /// The running app: id, version, build, data directory, and whether this run
    /// is an evidence run (and whether it is the build that replaced one).
    pub fn info(sys: System) -> String {
        answer((|| {
            let shipped = config();
            Ok(json!({
                "id": shipped.as_ref().map(|config| config.id.clone()),
                "version": shipped.as_ref().map(|config| config.version.clone()).unwrap_or_else(|| "dev".into()),
                "build": shipped.as_ref().and_then(|config| config.build),
                "source": shipped.as_ref().and_then(|config| config.source.as_ref()).map(|source| match (&source.github, &source.url) {
                    (Some(repo), _) => json!({"kind": "github", "value": repo}),
                    (_, Some(url)) => json!({"kind": "url", "value": url}),
                    _ => json!({"kind": "none"}),
                }),
                "data_dir": data_dir(&sys)?,
                "evidence": match evidence(&sys) {
                    Some(dir) => json!({
                        "active": true,
                        "dir": dir,
                        "restarted": restarted(&sys),
                    }),
                    None => json!({ "active": false }),
                },
            }))
        })())
    }

    /// Writes one evidence receipt: `<dir>/<step>.json`. Bounded and only while
    /// this run is an evidence run.
    pub fn receipt(sys: System, step: String, payload: String) -> String {
        answer((|| {
            let dir = evidence(&sys).ok_or("not an evidence run")?;
            if !valid_step(&step) {
                return Err("receipt step is not a short name".into());
            }
            if payload.len() > MAX_PAYLOAD_BYTES {
                return Err("receipt payload exceeds its budget".into());
            }
            let payload: Value =
                serde_json::from_str(&payload).map_err(|_| "receipt payload is not JSON")?;
            fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            let receipt = json!({"at_ms": now_ms(), "step": step, "payload": payload});
            let path = dir.join(format!("{step}.json"));
            fs::write(
                &path,
                serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            Ok(json!({"path": path}))
        })())
    }

    /// Arms the build that replaces this one: it reports into the same directory
    /// once it runs, then removes the marker with `consume`.
    pub fn arm(sys: System) -> String {
        answer((|| {
            let dir = evidence(&sys).ok_or("not an evidence run")?;
            let shipped = config();
            let marker = json!({
                "dir": dir,
                "from_version": shipped.as_ref().map(|config| config.version.clone()),
                "from_build": shipped.as_ref().and_then(|config| config.build),
                "at_ms": now_ms(),
            });
            let path = marker_path(&sys)?;
            fs::create_dir_all(data_dir(&sys)?).map_err(|error| error.to_string())?;
            fs::write(
                &path,
                serde_json::to_vec(&marker).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            Ok(json!({"path": path}))
        })())
    }

    /// Removes the marker once the restarted build reported its state.
    pub fn consume(sys: System) -> String {
        answer((|| {
            let path = marker_path(&sys)?;
            match fs::remove_file(&path) {
                Ok(()) => Ok(json!({"removed": path})),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(json!({"removed": null}))
                }
                Err(error) => Err(error.to_string()),
            }
        })())
    }

    /// Ends an evidence run the way quitting the app would. Only an evidence run
    /// may ask for it.
    pub fn finish(sys: System) -> String {
        answer((|| {
            evidence(&sys).ok_or("not an evidence run")?;
            nsh_core::element::desktop::quit();
            Ok(json!({"quitting": true}))
        })())
    }

    /// Exports the combined Core knowledge & vocabulary study archives into app backups.
    pub fn export_backup(sys: System) -> String {
        answer((|| {
            let store = crate::runtime::core_handle(&sys)?;
            export_backup_with_store(&sys, store)
        })())
    }

    /// Opens Android share sheet or desktop reveal for a generated backup file.
    pub fn share_backup(sys: System, path: String) -> String {
        answer((|| {
            let dir = backup_dir(&sys)?;
            let canonical_dir = fs::canonicalize(&dir)
                .map_err(|_| "尚未找到导出备份目录".to_string())?;
            let canonical_file = fs::canonicalize(PathBuf::from(&path))
                .map_err(|_| "找不到要分享的备份文件".to_string())?;
            if !canonical_file.starts_with(&canonical_dir) || !canonical_file.is_file() {
                return Err("备份文件路径不在应用备份目录中".into());
            }
            let file_str = canonical_file.to_string_lossy().to_string();
            #[cfg(target_os = "android")]
            {
                qusor_android_jni::share_file(&file_str, "application/json", "导出 QUSOR 备份")?;
                return Ok(json!({"shared": true, "path": file_str}));
            }
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("/usr/bin/open")
                    .args(["-R", &file_str])
                    .spawn()
                    .map_err(|e| format!("无法打开备份目录：{e}"))?;
                Ok(json!({"shared": true, "path": file_str}))
            }
            #[cfg(not(any(target_os = "android", target_os = "macos")))]
            {
                let _ = file_str;
                Err("this platform does not support native file sharing".into())
            }
        })())
    }

    /// Restores a combined or knowledge-only JSON backup into an empty Core.
    pub fn restore_backup(sys: System, document: String) -> String {
        answer((|| {
            let store = crate::runtime::core_handle(&sys)?;
            restore_backup_with_store(store, &document)
        })())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(age_ms: u64, version: &str, build: Option<u64>) -> Value {
        json!({
            "at_ms": 1_000_000u64.saturating_sub(age_ms),
            "from_version": version,
            "from_build": build,
        })
    }

    fn current(version: &str, build: Option<u64>) -> Config {
        Config {
            id: "org.qusor.app".into(),
            version: version.into(),
            build,
            source: None,
        }
    }

    #[test]
    fn live_marker_requires_a_fresh_other_build() {
        let current = current("0.1.1", Some(2));

        assert!(marker_is_live_at(
            &marker(10_000, "0.1.0", Some(1)),
            Some(&current),
            1_000_000
        ));
        assert!(marker_is_live_at(
            &marker(10_000, "0.1.1", Some(1)),
            Some(&current),
            1_000_000
        ));
        assert!(!marker_is_live_at(
            &marker(10_000, "0.1.1", Some(2)),
            Some(&current),
            1_000_000
        ));
        assert!(!marker_is_live_at(
            &marker(MARKER_MAX_AGE_MS + 1, "0.1.0", Some(1)),
            Some(&current),
            1_000_000
        ));
    }

    use crate::vault::VaultStore;
    use learning_application::{KnowledgeApplication, study::StudyApplication};
    use core_api::{
        contract::{
            CaptureDraft, ExternalReturn, NORMALIZATION, PROTOCOL_VERSION, SourceDescriptor,
            TEXT_UNIT, TextSelector,
        },
        knowledge::Provenance,
        vocab::{NewWord, StudyRating, WordbookSeed},
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "qusor-app-info-test-{}-{}",
                std::process::id(),
                NEXT_TEST.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn seed_knowledge_capture(store: &VaultStore, op: &str, quote: &str, note: &str) {
        let draft = CaptureDraft {
            protocol_version: PROTOCOL_VERSION,
            source: SourceDescriptor {
                namespace: "local".into(),
                reference: "doc-1".into(),
                title: "Test Doc".into(),
                public_uri: None,
                external_return: ExternalReturn::None,
            },
            snapshot_text: quote.into(),
            selector: TextSelector {
                unit: TEXT_UNIT.into(),
                normalization: NORMALIZATION.into(),
                start: Some(0),
                end: Some(quote.chars().count() as u64),
                exact: quote.into(),
            },
        };
        let provenance = Provenance {
            package_id: "org.qusor.notes".into(),
            publisher_id: "org.qusor".into(),
            package_digest: "a".repeat(64),
            installation_id: 1,
        };
        KnowledgeApplication::new(store.clone())
            .commit(op, "qusor-vault", &draft, &provenance, note)
            .unwrap();
    }

    #[test]
    fn backup_export_and_restore_roundtrip() {
        let src_dir = Scratch::new();
        let mut src_sys = System::default();
        src_sys.data_dir = Some(src_dir.0.join("data"));
        let src_store = VaultStore::open_app(&src_dir.0.join("data")).unwrap();
        src_store.ensure_starter_wordbook().unwrap();

        seed_knowledge_capture(&src_store, "op-backup-1", "Backup quote text", "My backup note");

        let mut study_app = StudyApplication::new(src_store.clone());
        let book = study_app
            .seed_wordbook(
                "qusor-vault",
                &WordbookSeed {
                    title: "Custom Deck".into(),
                    description: "For backup test".into(),
                    words: vec![NewWord {
                        term: "resilient".into(),
                        pos: "adj.".into(),
                        phonetic: "/rɪˈzɪliənt/".into(),
                        meaning: "有韧性的".into(),
                        example: "A resilient backup restores cleanly.".into(),
                    }],
                },
                1_767_225_600_000,
            )
            .unwrap();
        let queue = study_app
            .queue("qusor-vault", book.id, 1_767_225_601_000, 1_767_225_600_000, 10)
            .unwrap();
        let card_id = queue.cards[0].card.id;
        study_app
            .review(
                "qusor-vault",
                card_id,
                StudyRating::Good,
                "rev-op-1",
                1_767_225_602_000,
                1_767_225_600_000,
            )
            .unwrap();

        let exported = export_backup_with_store(&src_sys, src_store.clone()).unwrap();
        let path = PathBuf::from(exported["path"].as_str().unwrap());
        assert!(path.is_file());
        let json_str = exported["json"].as_str().unwrap();
        assert_eq!(exported["bytes"].as_u64().unwrap() as usize, json_str.len());
        let parsed: Value = serde_json::from_str(json_str).unwrap();
        assert_eq!(parsed["format"], BACKUP_FORMAT);

        // Fresh Core with auto-seeded starter wordbook should cleanly replace starter and restore.
        let dst_dir = Scratch::new();
        let dst_store = VaultStore::open_app(&dst_dir.0.join("data")).unwrap();
        dst_store.ensure_starter_wordbook().unwrap();

        let receipt = restore_backup_with_store(dst_store.clone(), json_str).unwrap();
        assert_eq!(receipt["text_captures"], 1);
        assert_eq!(receipt["wordbooks"], 2);

        let restored_knowledge = KnowledgeApplication::new(dst_store.clone())
            .export_all()
            .unwrap();
        assert_eq!(restored_knowledge.text_entries.len(), 1);
        assert_eq!(restored_knowledge.text_entries[0].record.user_note, "My backup note");

        let restored_study = StudyApplication::new(dst_store).export_study().unwrap();
        assert_eq!(restored_study.wordbooks.len(), 2);
        assert_eq!(restored_study.reviews.len(), 1);
        assert!(
            restored_study
                .wordbooks
                .iter()
                .any(|wb| wb.title == "Custom Deck")
        );

        // Standalone ArchiveV2 backup also restores into an empty Core.
        let standalone_dir = Scratch::new();
        let standalone_store = VaultStore::open_app(&standalone_dir.0.join("data")).unwrap();
        let standalone_json = serde_json::to_string(&restored_knowledge).unwrap();
        let standalone_receipt =
            restore_backup_with_store(standalone_store.clone(), &standalone_json).unwrap();
        assert_eq!(standalone_receipt["text_captures"], 1);
        assert_eq!(standalone_receipt["wordbooks"], 0);
    }

    #[test]
    fn restore_rejects_non_empty_core_and_invalid_documents() {
        let src_dir = Scratch::new();
        let mut src_sys = System::default();
        src_sys.data_dir = Some(src_dir.0.join("data"));
        let src_store = VaultStore::open_app(&src_dir.0.join("data")).unwrap();
        seed_knowledge_capture(&src_store, "op-src", "Source quote", "Source note");
        let backup_json = export_backup_with_store(&src_sys, src_store)
            .unwrap()["json"]
            .as_str()
            .unwrap()
            .to_string();

        // 1) Invalid format / malformed JSON rejected before touching Core.
        let empty_dir = Scratch::new();
        let empty_store = VaultStore::open_app(&empty_dir.0.join("data")).unwrap();
        assert!(restore_backup_with_store(empty_store.clone(), "{not json").is_err());
        let bad_format = backup_json.replace(BACKUP_FORMAT, "qusor-backup-v999");
        assert!(restore_backup_with_store(empty_store.clone(), &bad_format).is_err());
        assert!(
            KnowledgeApplication::new(empty_store)
                .export_all()
                .unwrap()
                .commits
                .is_empty()
        );

        // 2) Core with existing knowledge commit rejects restore and preserves existing capture.
        let populated_dir = Scratch::new();
        let populated_store = VaultStore::open_app(&populated_dir.0.join("data")).unwrap();
        seed_knowledge_capture(&populated_store, "op-existing", "Existing", "Keep me");
        let err = restore_backup_with_store(populated_store.clone(), &backup_json).unwrap_err();
        assert!(err.contains("restore requires empty Core"), "{err}");
        let preserved = KnowledgeApplication::new(populated_store)
            .export_all()
            .unwrap();
        assert_eq!(preserved.text_entries.len(), 1);
        assert_eq!(preserved.text_entries[0].record.user_note, "Keep me");

        // 3) Core with user wordbook rejects combined restore and preserves existing wordbook.
        let vocab_dir = Scratch::new();
        let vocab_store = VaultStore::open_app(&vocab_dir.0.join("data")).unwrap();
        StudyApplication::new(vocab_store.clone())
            .seed_wordbook(
                "qusor-vault",
                &WordbookSeed {
                    title: "User Book".into(),
                    description: String::new(),
                    words: vec![NewWord {
                        term: "keep".into(),
                        pos: String::new(),
                        phonetic: String::new(),
                        meaning: "保留".into(),
                        example: String::new(),
                    }],
                },
                1_767_225_600_000,
            )
            .unwrap();
        let err = restore_backup_with_store(vocab_store.clone(), &backup_json).unwrap_err();
        assert!(err.contains("restore requires empty Core"), "{err}");
        let study_after = StudyApplication::new(vocab_store).export_study().unwrap();
        assert_eq!(study_after.wordbooks.len(), 1);
        assert_eq!(study_after.wordbooks[0].title, "User Book");
    }
}
