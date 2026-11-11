//! Workbench vocabulary host: an ordinary signed package, the public
//! vocab-study-v1 wire and a real Core database. Startup input is environment
//! only; TSX never sees a database path, a package key or Host authority.
use extension_api::{vocab::*, *};
use extension_host::{Grant, Host, Session};
use extension_package::package::verify_for_host;
use learning_application::study::StudyApplication;
use nsh_core::prelude::*;
use qusor_capture_probe::vocab_bridge::VocabBridge;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use storage_sqlite::SqliteStore;

struct Runtime {
    host: Host,
    session: Session,
    bridge: VocabBridge<SqliteStore>,
}
static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

/// Startup-only wiring. A missing environment leaves the module unavailable so
/// the workbench can keep its fixture for headless tests.
pub fn initialize_from_env() -> Result<(), String> {
    let Ok(data) = std::env::var("QUSOR_VOCAB_DATA") else {
        return Ok(());
    };
    let package = std::env::var("QUSOR_VOCAB_PACKAGE").map_err(text)?;
    let key = std::env::var("QUSOR_VOCAB_KEY").map_err(text)?;
    initialize(Path::new(&data), Path::new(&package), Path::new(&key))
}

/// Trusted startup input: a seeded Core directory, the ordinary signed vocab
/// package and its trusted public key file.
pub fn initialize(data: &Path, package: &Path, key: &Path) -> Result<(), String> {
    let bytes = fs::read(package).map_err(text)?;
    let trusted = fs::read_to_string(key).map_err(text)?;
    let verified = verify_for_host(&bytes, trusted.trim(), &SUPPORTED_HOST_APIS).map_err(text)?;
    let mut host = Host::new(
        "workbench-vault",
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
                    vault_id: "workbench-vault".into(),
                },
            },
            Grant {
                capability: Capability::VocabReview,
                resource: extension_api::Resource::Vault {
                    vault_id: "workbench-vault".into(),
                },
            },
        ],
    )
    .map_err(text)?;
    host.enable(installation).map_err(text)?;
    let session = host
        .open_session_for_api(installation, HOST_API_V0_5)
        .map_err(text)?;
    let app = StudyApplication::new(SqliteStore::open(data).map_err(text)?);
    let now = now_ms();
    let bridge = VocabBridge::new(app, "workbench-vault", now, day_start_ms(now));
    *RUNTIME.lock().map_err(text)? = Some(Runtime {
        host,
        session,
        bridge,
    });
    Ok(())
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

/// The normal host initializes at startup; a test host may only call the
/// module, so initialization is also lazy when the environment is present.
fn ensure_runtime() -> Result<(), String> {
    if RUNTIME.lock().map_err(text)?.is_some() {
        return Ok(());
    }
    initialize_from_env()
}

fn answer(f: impl FnOnce(&mut Runtime) -> Result<Value, String>) -> String {
    if let Err(error) = ensure_runtime() {
        return json!({"ok": false, "error": error}).to_string();
    }
    let value = match RUNTIME.lock() {
        Ok(mut guard) => match guard.as_mut() {
            Some(runtime) => {
                let now = now_ms();
                runtime.bridge.set_clock(now, day_start_ms(now));
                match f(runtime) {
                    Ok(value) => json!({"ok": true, "value": value}),
                    Err(error) => json!({"ok": false, "error": error}),
                }
            }
            None => json!({"ok": false, "error": "vocab runtime unavailable"}),
        },
        Err(_) => json!({"ok": false, "error": "vocab runtime unavailable"}),
    };
    value.to_string()
}

fn wire_error(error: ApiError) -> String {
    format!("{:?}: {}", error.code, error.message)
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
/// Speaking rate of the sample service: a little slower than the system default.
const SPEECH_RATE_NORMAL: &str = "165";
/// Slower speaking rate learners can pick for practice.
const SPEECH_RATE_SLOW: &str = "120";

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

/// The words-per-minute argument for a requested reading speed.
fn speech_rate(rate: &str) -> Result<&'static str, String> {
    match rate {
        "normal" => Ok(SPEECH_RATE_NORMAL),
        "slow" => Ok(SPEECH_RATE_SLOW),
        _ => Err("unknown pronunciation rate".into()),
    }
}

/// Rejects speech the system voice must not be asked to read.
fn validate_speech(text: &str, accent: &str, rate: &str) -> Result<(), String> {
    if text.is_empty() || text.len() > MAX_SPEECH_BYTES {
        return Err(format!("speech text must be 1 to {MAX_SPEECH_BYTES} bytes"));
    }
    if text.chars().any(|ch| ch.is_control() && ch != '\n') {
        return Err("speech text contains control characters".into());
    }
    if accent != "us" && accent != "uk" {
        return Err("unknown pronunciation accent".into());
    }
    speech_rate(rate)?;
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
fn speak_now(text: &str, accent: &str, rate: &str) -> Result<Option<String>, String> {
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
    command.args(["-r", speech_rate(rate)?]);
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
fn speak_now(_text: &str, _accent: &str, _rate: &str) -> Result<Option<String>, String> {
    Err("this platform's host has no system voice".into())
}

/// One in-process render capture of the app window. This is the same kind of
/// evidence the NSH test harness takes: it needs no screen-recording grant and
/// does not depend on which Space or window is frontmost.
#[cfg(target_os = "macos")]
fn capture_window(path: &Path) -> Result<(), String> {
    use objc2::MainThreadMarker;
    if MainThreadMarker::new().is_none() {
        let path = path.to_path_buf();
        let (sender, receiver) = std::sync::mpsc::channel();
        dispatch2::DispatchQueue::main().exec_async(move || {
            let _ = sender.send(capture_on_main(&path));
        });
        return receiver
            .recv()
            .map_err(|_| "main thread capture did not answer".to_string())?;
    }
    capture_on_main(path)
}

#[cfg(target_os = "macos")]
fn capture_on_main(path: &Path) -> Result<(), String> {
    use objc2::MainThreadMarker;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSApplication, NSBitmapImageFileType};
    use objc2_foundation::{NSDictionary, NSString};
    let marker = MainThreadMarker::new().ok_or("capture must run on the main thread")?;
    let application = NSApplication::sharedApplication(marker);
    let window = application
        .keyWindow()
        .or_else(|| application.windows().iter().next())
        .ok_or("no app window is open")?;
    let view = window.contentView().ok_or("no content view")?;
    let bounds = view.bounds();
    let bitmap = view
        .bitmapImageRepForCachingDisplayInRect(bounds)
        .ok_or("cannot allocate a window bitmap")?;
    view.cacheDisplayInRect_toBitmapImageRep(bounds, &bitmap);
    let properties: objc2::rc::Retained<NSDictionary<_, AnyObject>> = NSDictionary::new();
    let data = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
    }
    .ok_or("cannot encode the window bitmap")?;
    let target = NSString::from_str(path.to_str().ok_or("capture path must be UTF-8")?);
    if !data.writeToFile_atomically(&target, true) {
        return Err("cannot write the window capture".into());
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn capture_window(_path: &Path) -> Result<(), String> {
    Err("window capture is only wired for macOS".into())
}

#[module]
pub struct VocabHost;

#[module]
impl VocabHost {
    /// Wordbooks and their total/due/new counters from the package-authorized Core.
    pub fn decks() -> String {
        answer(|runtime| {
            let request = VocabRequest {
                vocab_protocol: VOCAB_PROTOCOL,
                api_version: HOST_API_V0_5,
                request_id: "workbench-decks".into(),
                operation: VocabOperation::Decks {},
            };
            let Runtime {
                host,
                session,
                bridge,
            } = runtime;
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
        })
    }

    /// One bounded study queue page: due reviews first, then new cards.
    pub fn queue(wordbook_id: i64, limit: u16) -> String {
        answer(|runtime| {
            let request = VocabRequest {
                vocab_protocol: VOCAB_PROTOCOL,
                api_version: HOST_API_V0_5,
                request_id: "workbench-queue".into(),
                operation: VocabOperation::Queue { wordbook_id, limit },
            };
            let Runtime {
                host,
                session,
                bridge,
            } = runtime;
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
        })
    }

    /// Commit one graded review through the public wire. `operation_id` is the
    /// caller's user-action key: repeating it must not schedule the card twice.
    pub fn review(card_id: i64, rating: String, operation_id: String) -> String {
        answer(|runtime| {
            let rating = parse_rating(&rating)?;
            let request = VocabRequest {
                vocab_protocol: VOCAB_PROTOCOL,
                api_version: HOST_API_V0_5,
                request_id: operation_id,
                operation: VocabOperation::Review { card_id, rating },
            };
            let Runtime {
                host,
                session,
                bridge,
            } = runtime;
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
        })
    }

    /// Read one short text (a term or its example) with the system voice.
    /// The sample host asks the platform's voice; no pronunciation data is
    /// invented here and nothing is written to Core.
    pub fn pronounce(text: String, accent: String, rate: String) -> String {
        if let Err(error) = validate_speech(&text, &accent, &rate) {
            return json!({ "ok": false, "error": error }).to_string();
        }
        // The sample host serves a workspace that already runs on the real
        // package/Core runtime: a window without it keeps the sample card and
        // says it cannot read instead of starting the system voice anyway.
        answer(|_| {
            speak_now(&text, &accent, &rate)
                .map(|voice| json!({ "accent": accent, "rate": rate, "voice": voice }))
        })
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

    /// Evidence-only: capture the current app window into a PNG.
    pub fn capture(path: String) -> String {
        match capture_window(Path::new(&path)) {
            Ok(()) => json!({"ok": true, "value": {"path": path}}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = "Fred                en_US    # Hello! My name is Fred.\n\
                           Samantha (英语（美国）)   en_US    # Hello! My name is Samantha.\n\
                           Daniel (英语（英国）)     en_GB    # Hello! My name is Daniel.\n\
                           Eddy (英语（美国）)       en_US    # Hello! My name is Eddy.\n\
                           Amélie              fr_CA    # Bonjour! Je m’appelle Amélie.\n";

    #[test]
    fn a_voice_listing_keeps_names_with_spaces() {
        let voices = parse_voice_listing(LISTING);
        assert!(voices.contains(&("Eddy (英语（美国）)".to_string(), "en_US".to_string())));
        assert!(voices.contains(&("Daniel (英语（英国）)".to_string(), "en_GB".to_string())));
        assert!(!voices.iter().any(|(voice, _)| voice == "Bonjour!"));
        assert!(voice_named("Samantha (英语（美国）)", "Samantha"));
        assert!(!voice_named("Samantha", "Sam"));
    }

    #[test]
    fn an_accent_prefers_its_known_voice_then_any_voice_of_the_locale() {
        let voices = parse_voice_listing(LISTING);
        assert_eq!(speech_voice("us", &voices), Some("Samantha".to_string()));
        assert_eq!(speech_voice("uk", &voices), Some("Daniel".to_string()));
        assert_eq!(
            speech_voice("us", &[("Fred".to_string(), "en_US".to_string())]),
            Some("Fred".to_string())
        );
        let only_uk = vec![("Extra".to_string(), "en_GB".to_string())];
        assert_eq!(speech_voice("uk", &only_uk), Some("Extra".to_string()));
        assert_eq!(speech_voice("us", &only_uk), None);
    }

    #[test]
    fn speech_validation_rejects_bad_text_accent_and_rate() {
        assert!(validate_speech("abandon", "us", "normal").is_ok());
        assert!(validate_speech("They had to abandon the plan.", "uk", "slow").is_ok());
        assert!(validate_speech("", "us", "normal").is_err());
        assert!(validate_speech("abandon", "au", "normal").is_err());
        assert!(validate_speech("abandon", "us", "fastest").is_err());
        assert!(validate_speech("a\u{7}b", "us", "normal").is_err());
        assert!(validate_speech(&"a".repeat(MAX_SPEECH_BYTES + 1), "us", "normal").is_err());
        assert_eq!(speech_rate("slow"), Ok("120"));
        assert_eq!(speech_rate("normal"), Ok("165"));
        assert!(speech_rate("fast").is_err());
    }
}
