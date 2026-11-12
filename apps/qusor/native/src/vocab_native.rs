use crate::{package_catalog, runtime, vault::VaultStore};
use core_api::vocab::{NewWord, WordbookSeed};
use extension_api::{vocab::*, *};
use extension_host::{Grant, Host, Session};
use learning_application::study::StudyApplication;
use nsh_core::prelude::*;
use qusor_app_bridges::vocab_bridge::VocabBridge;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct VocabRuntime {
    host: Host,
    session: Session,
    store: VaultStore,
    bridge: VocabBridge<VaultStore>,
}

impl VocabRuntime {
    pub(crate) fn mount(store: VaultStore) -> Result<Self, String> {
        crate::profile::require_active(crate::profile::VOCAB_HOST_ENTRY)?;
        let verified = package_catalog::verify("org.qusor.vocab")?;
        let mut host = Host::new(
            "qusor-vault",
            [Capability::VocabRead, Capability::VocabReview],
        )
        .map_err(text)?;
        let installation = host
            .register(verified.manifest().clone(), verified.digest())
            .map_err(text)?;
        host.replace_grants(
            installation,
            vec![
                Grant {
                    capability: Capability::VocabRead,
                    resource: extension_api::Resource::Vault {
                        vault_id: "qusor-vault".into(),
                    },
                },
                Grant {
                    capability: Capability::VocabReview,
                    resource: extension_api::Resource::Vault {
                        vault_id: "qusor-vault".into(),
                    },
                },
            ],
        )
        .map_err(text)?;
        host.enable(installation).map_err(text)?;
        let session = host
            .open_session_for_api(installation, HOST_API_V0_5)
            .map_err(text)?;
        let now = now_ms();
        Ok(Self {
            host,
            session,
            store: store.clone(),
            bridge: VocabBridge::new(
                StudyApplication::new(store),
                "qusor-vault",
                now,
                day_start_ms(now),
            ),
        })
    }

    fn clock(&mut self) {
        let now = now_ms();
        self.bridge.set_clock(now, day_start_ms(now));
    }

    fn decks(&mut self) -> Result<Value, String> {
        self.clock();
        let request = VocabRequest {
            vocab_protocol: VOCAB_PROTOCOL,
            api_version: HOST_API_V0_5,
            request_id: "workbench-decks".into(),
            operation: VocabOperation::Decks {},
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_vocab(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |_context| Ok(bridge.respond(&request)))
            .map_err(text)?;
        match response.result {
            VocabResult::Decks { decks } => Ok(json!({ "decks": decks })),
            VocabResult::Queue { .. } => Err("unexpected queue response".into()),
            VocabResult::Review(_) => Err("unexpected review response".into()),
            VocabResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn queue(&mut self, wordbook_id: i64, limit: u16) -> Result<Value, String> {
        self.clock();
        let request = VocabRequest {
            vocab_protocol: VOCAB_PROTOCOL,
            api_version: HOST_API_V0_5,
            request_id: "workbench-queue".into(),
            operation: VocabOperation::Queue { wordbook_id, limit },
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_vocab(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |_context| Ok(bridge.respond(&request)))
            .map_err(text)?;
        match response.result {
            VocabResult::Queue {
                cards,
                remaining_due,
                remaining_new,
            } => Ok(json!({
                "cards": cards,
                "remaining_due": remaining_due,
                "remaining_new": remaining_new,
            })),
            VocabResult::Decks { .. } => Err("unexpected decks response".into()),
            VocabResult::Review(_) => Err("unexpected review response".into()),
            VocabResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn review(
        &mut self,
        card_id: i64,
        rating: String,
        operation_id: String,
    ) -> Result<Value, String> {
        self.clock();
        let rating = parse_rating(&rating)?;
        let request = VocabRequest {
            vocab_protocol: VOCAB_PROTOCOL,
            api_version: HOST_API_V0_5,
            request_id: operation_id,
            operation: VocabOperation::Review { card_id, rating },
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_vocab(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |_context| Ok(bridge.respond(&request)))
            .map_err(text)?;
        match response.result {
            VocabResult::Review(outcome) => Ok(json!({
                "card_id": outcome.card_id,
                "stage": outcome.stage,
                "due_at_ms": outcome.due_at_ms,
                "scheduled_minutes": outcome.scheduled_minutes,
                "remaining_due": outcome.remaining_due,
                "remaining_new": outcome.remaining_new,
            })),
            VocabResult::Decks { .. } => Err("unexpected decks response".into()),
            VocabResult::Queue { .. } => Err("unexpected queue response".into()),
            VocabResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn import_wordbook(&mut self, document: String) -> Result<Value, String> {
        let seed = parse_import_wordbook(&document)?;
        let title = seed.title.clone();
        let total = seed.words.len();
        let now = now_ms();
        let mut study = StudyApplication::new(self.store.clone());
        if study
            .decks("qusor-vault", now)
            .map_err(text)?
            .iter()
            .any(|deck| deck.wordbook.title == title)
        {
            return Err("a wordbook with this title already exists".into());
        }
        let wordbook = study
            .seed_wordbook("qusor-vault", &seed, now)
            .map_err(text)?;
        Ok(json!({
            "wordbook_id": wordbook.id,
            "title": wordbook.title,
            "total": total,
        }))
    }
}

const MAX_WORDBOOK_IMPORT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportWordbook {
    title: String,
    description: Option<String>,
    words: Vec<ImportWord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportWord {
    term: String,
    meaning: String,
    example: Option<String>,
}

fn parse_import_wordbook(document: &str) -> Result<WordbookSeed, String> {
    if document.is_empty() || document.len() > MAX_WORDBOOK_IMPORT_BYTES {
        return Err(format!(
            "wordbook JSON must be 1 to {MAX_WORDBOOK_IMPORT_BYTES} bytes"
        ));
    }
    let imported: ImportWordbook =
        serde_json::from_str(document).map_err(|_| "invalid wordbook JSON".to_string())?;
    let title = imported.title.trim().to_string();
    if title.is_empty() {
        return Err("wordbook title is required".into());
    }
    Ok(WordbookSeed {
        title,
        description: imported.description.unwrap_or_default(),
        words: imported
            .words
            .into_iter()
            .map(|word| NewWord {
                term: word.term,
                meaning: word.meaning,
                example: word.example.unwrap_or_default(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn import_shape_defaults_optional_fields() {
        let seed = parse_import_wordbook(
            r#"{"title":"  My words  ","words":[{"term":"term","meaning":"词"}]}"#,
        )
        .unwrap();
        assert_eq!(seed.title, "My words");
        assert_eq!(seed.description, "");
        assert_eq!(seed.words[0].example, "");
    }

    #[test]
    fn import_rejects_unknown_empty_or_oversized_input() {
        assert_eq!(
            parse_import_wordbook(r#"{"title":"x","words":[],"path":"/tmp/a"}"#).unwrap_err(),
            "invalid wordbook JSON"
        );
        assert_eq!(
            parse_import_wordbook(r#"{"title":"  ","words":[]}"#).unwrap_err(),
            "wordbook title is required"
        );
        assert!(
            parse_import_wordbook(&" ".repeat(MAX_WORDBOOK_IMPORT_BYTES + 1))
                .unwrap_err()
                .contains("must be 1 to")
        );
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| i64::try_from(value.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(1)
}

/// Local midnight is supplied by the trusted launcher; the UTC fallback keeps a
/// manually started probe deterministic enough for review.
fn day_start_ms(now: i64) -> i64 {
    std::env::var("QUSOR_VOCAB_DAY_START_MS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0 && *value <= now)
        .unwrap_or(now - now.rem_euclid(86_400_000))
}

fn wire_error(error: ApiError) -> String {
    format!("{:?}: {}", error.code, error.message)
}

fn answer(value: Result<Value, String>) -> String {
    match value {
        Ok(value) => json!({"ok": true, "value": value}).to_string(),
        Err(error) => json!({"ok": false, "error": error}).to_string(),
    }
}

fn parse_rating(value: &str) -> Result<VocabRating, String> {
    match value {
        "again" => Ok(VocabRating::Again),
        "hard" => Ok(VocabRating::Hard),
        "good" => Ok(VocabRating::Good),
        "easy" => Ok(VocabRating::Easy),
        _ => Err("unknown vocab rating".into()),
    }
}

// ---------------------------------------------------------------------------
// Sample pronunciation service.
//
// The 0.5 wire carries terms, not recordings: the plugin owns no audio and the
// Core stores no pronunciation. Until a wordbook import brings its own audio,
// this experimental host asks the platform's system voice to read the term (or
// its example) the Core returned, so the study card can be heard offline. Only
// the reading itself is platform work; nothing is written to Core and no
// phonetic is invented here.
// ---------------------------------------------------------------------------

/// System voices the sample service prefers for the American reading.
const SPEECH_US_VOICES: &[&str] = &["Samantha", "Ava", "Allison", "Alex", "Fred"];
/// System voices it prefers for the British reading.
const SPEECH_UK_VOICES: &[&str] = &["Daniel", "Kate", "Serena", "Oliver"];
/// Longest text one pronunciation call may hand to the system voice.
const MAX_SPEECH_BYTES: usize = 400;
/// Speaking rate of the sample service, slightly slower than the default.
const SPEECH_RATE: &str = "165";

/// The locale a voice must have to read the selected accent.
fn speech_language(accent: &str) -> &'static str {
    if accent == "uk" { "en_GB" } else { "en_US" }
}

/// Reads the lines of `say -v '?'`: a padded voice name, its locale and an
/// example after `#`. Names may contain spaces ("Bad News"), so the locale is
/// the last whitespace-separated token before the comment.
fn parse_voice_listing(listing: &str) -> Vec<(String, String)> {
    let mut voices = Vec::new();
    for line in listing.lines() {
        let Some((left, _)) = line.split_once('#') else {
            continue;
        };
        let left = left.trim();
        let Some((name, locale)) = left.rsplit_once(|ch: char| ch.is_whitespace()) else {
            continue;
        };
        let (name, locale) = (name.trim(), locale.trim());
        if !name.is_empty() && !locale.is_empty() {
            voices.push((name.to_string(), locale.to_string()));
        }
    }
    voices
}

/// A listed voice matches a preferred name when it is that name or that name
/// with the system's localized suffix ("Samantha (英语（美国）)").
fn voice_named(voice: &str, name: &str) -> bool {
    voice == name || voice.starts_with(name) && voice[name.len()..].trim_start().starts_with('(')
}

/// The installed voice for an accent: a preferred name when it is installed,
/// else any voice of the locale, else `None` (the system's own default voice).
fn speech_voice(accent: &str, installed: &[(String, String)]) -> Option<String> {
    let preferred = if accent == "uk" {
        SPEECH_UK_VOICES
    } else {
        SPEECH_US_VOICES
    };
    let language = speech_language(accent);
    preferred
        .iter()
        .find(|candidate| {
            installed
                .iter()
                .any(|(voice, locale)| voice_named(voice, candidate) && locale == language)
        })
        .map(|candidate| (*candidate).to_string())
        .or_else(|| {
            installed
                .iter()
                .find(|(_, locale)| locale == language)
                .map(|(voice, _)| voice.clone())
        })
}

/// Rejects speech the system voice must not be asked to read.
fn validate_speech(text: &str, accent: &str) -> Result<(), String> {
    if text.is_empty() || text.len() > MAX_SPEECH_BYTES {
        return Err(format!("speech text must be 1 to {MAX_SPEECH_BYTES} bytes"));
    }
    if text.chars().any(|ch| ch.is_control() && ch != '\n') {
        return Err("speech text contains control characters".into());
    }
    if accent != "us" && accent != "uk" {
        return Err("unknown pronunciation accent".into());
    }
    Ok(())
}

/// The voices this machine installed, asked once per process.
#[cfg(target_os = "macos")]
fn installed_voices() -> &'static [(String, String)] {
    use std::sync::OnceLock;
    static VOICES: OnceLock<Vec<(String, String)>> = OnceLock::new();
    VOICES.get_or_init(|| {
        match std::process::Command::new("/usr/bin/say")
            .args(["-v", "?"])
            .output()
        {
            Ok(output) => parse_voice_listing(&String::from_utf8_lossy(&output.stdout)),
            Err(_) => Vec::new(),
        }
    })
}

/// Reads the text with the system voice. The process keeps speaking after this
/// call returns; a new call stops the previous reading instead of stacking.
#[cfg(target_os = "macos")]
fn speak_now(text: &str, accent: &str) -> Result<Option<String>, String> {
    use std::{
        io::Write,
        process::{Child, Command, Stdio},
        sync::Mutex,
    };
    static SPEAKER: Mutex<Option<Child>> = Mutex::new(None);
    if let Ok(mut speaker) = SPEAKER.lock() {
        if let Some(mut previous) = speaker.take() {
            let _ = previous.kill();
            let _ = previous.try_wait();
        }
    }
    let voice = speech_voice(accent, installed_voices());
    let mut command = Command::new("/usr/bin/say");
    command.args(["-r", SPEECH_RATE]);
    if let Some(voice) = &voice {
        command.args(["-v", voice]);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start the system voice: {error}"))?;
    let handed = match child.stdin.take() {
        Some(mut stdin) => stdin.write_all(text.as_bytes()),
        None => Ok(()),
    };
    handed.map_err(|error| format!("cannot hand the text to the system voice: {error}"))?;
    if let Ok(mut speaker) = SPEAKER.lock() {
        *speaker = Some(child);
    }
    Ok(voice)
}

#[cfg(not(target_os = "macos"))]
fn speak_now(_text: &str, _accent: &str) -> Result<Option<String>, String> {
    Err("this platform's host has no system voice".into())
}

/// Native Host surface for bounded, package-authorized vocabulary study.
#[module]
pub struct VocabHost;

#[module]
impl VocabHost {
    /// Wordbooks and their total/due/new counters from the package-authorized Core.
    pub fn decks(sys: System) -> String {
        answer(runtime::with_vocab(&sys, VocabRuntime::decks))
    }

    /// One bounded study queue page: due reviews first, then new cards.
    pub fn queue(sys: System, wordbook_id: i64, limit: u16) -> String {
        answer(runtime::with_vocab(&sys, |vocab| {
            vocab.queue(wordbook_id, limit)
        }))
    }

    /// Commit one graded review through the public wire. `operation_id` is the
    /// caller's user-action key: repeating it must not schedule the card twice.
    pub fn review(sys: System, card_id: i64, rating: String, operation_id: String) -> String {
        answer(runtime::with_vocab(&sys, |vocab| {
            vocab.review(card_id, rating, operation_id)
        }))
    }

    /// Import a user-selected, bounded JSON wordbook through the trusted app
    /// runtime. This is a host import action; study reads and reviews continue
    /// through the ordinary package capability path.
    pub fn import_wordbook(sys: System, document: String) -> String {
        answer(runtime::with_vocab(&sys, |vocab| {
            vocab.import_wordbook(document)
        }))
    }

    /// Read one short text (a term or its example) with the system voice.
    /// The sample host asks the platform's voice; no pronunciation data is
    /// invented here and nothing is written to Core.
    pub fn pronounce(sys: System, text: String, accent: String) -> String {
        if let Err(error) = validate_speech(&text, &accent) {
            return json!({ "ok": false, "error": error }).to_string();
        }
        // The sample host serves a workspace that already runs on the real
        // package/Core runtime: a window without it keeps the sample card and
        // says it cannot read instead of starting the system voice anyway.
        answer(runtime::with_vocab(&sys, |_| {
            speak_now(&text, &accent).map(|voice| json!({ "accent": accent, "voice": voice }))
        }))
    }

    /// Evidence-only: report whether the host was started for capture and which
    /// workspace the launcher asked for.
    pub fn evidence() -> String {
        let active = std::env::var_os("QUSOR_WORKBENCH_EVIDENCE").is_some();
        let dir = std::env::var("QUSOR_WORKBENCH_CAPTURE_DIR").unwrap_or_default();
        let target = std::env::var("QUSOR_WORKBENCH_EVIDENCE_TARGET")
            .unwrap_or_else(|_| "notes".to_string());
        json!({"ok": true, "value": {"active": active, "dir": dir, "target": target}}).to_string()
    }

    /// Evidence-only: a packaged build has no in-process capture path.
    pub fn capture(path: String) -> String {
        json!({"ok": false, "error": "window capture is not available in this build", "path": path})
            .to_string()
    }
}
