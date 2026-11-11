//! End-to-end: an ordinary signed vocabulary package from `plugins/vocab` is
//! built outside the repository, verified, installed into the Host, granted
//! `vocab.read`/`vocab.review`, executed by the Wasm runtime, and answered by the
//! real SQLite Core through the trusted bridge.
#![cfg(unix)]

use core_api::{
    contract::sha256,
    vocab::{NewWord, StudyRating, StudyStage, WordbookSeed},
};
use extension_api::{vocab::*, *};
use extension_host::{Grant, Host};
use extension_package::package::{
    PackageFile, VerifiedPackage, build_signed_for_host, publisher_key, verify_for_host,
};
use learning_application::study::StudyApplication;
use qusor_capture_probe::vocab_bridge::VocabBridge;
use runtime_wasm::{RuntimeError, run_managed_raw};
use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

const MANIFEST: &str = include_str!("../../../plugins/vocab/manifest.json");
const GUEST: &str = include_str!("../../../plugins/vocab/guest.wat");
const SEED: [u8; 32] = [0x33; 32];
const NOW: i64 = 1_767_225_600_000;
const DAY: i64 = 86_400_000;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-vocab-package-{}-{}",
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

fn seed() -> WordbookSeed {
    WordbookSeed {
        title: "CET-4 sample".into(),
        description: "Synthetic three-word fixture, not a licensed word list.".into(),
        words: vec![
            NewWord {
                term: "abandon".into(),
                meaning: "放弃".into(),
                example: "Do not abandon the plan.".into(),
            },
            NewWord {
                term: "benefit".into(),
                meaning: "益处".into(),
                example: "Reading has many benefits.".into(),
            },
            NewWord {
                term: "candidate".into(),
                meaning: "候选人".into(),
                example: "The candidate answered well.".into(),
            },
        ],
    }
}

fn setup_core() -> (Scratch, StudyApplication<SqliteStore>, i64) {
    let scratch = Scratch::new();
    let mut app = StudyApplication::new(SqliteStore::open(&scratch.0).unwrap());
    let book = app.seed_wordbook("vault", &seed(), NOW).unwrap();
    (scratch, app, book.id)
}

fn signed_package() -> (VerifiedPackage, Vec<u8>) {
    let manifest = Manifest::from_slice(MANIFEST.as_bytes()).unwrap();
    let wasm = wat::parse_str(GUEST).unwrap();
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
    let verified = verify_for_host(&bytes, &publisher_key(&SEED), &SUPPORTED_HOST_APIS).unwrap();
    (verified, bytes)
}

fn read_only_package() -> VerifiedPackage {
    let mut manifest = Manifest::from_slice(MANIFEST.as_bytes()).unwrap();
    manifest.required_capabilities = vec![Capability::VocabRead];
    manifest.optional_capabilities = vec![Capability::VocabReview];
    let wasm = wat::parse_str(GUEST).unwrap();
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
    verify_for_host(&bytes, &publisher_key(&SEED), &SUPPORTED_HOST_APIS).unwrap()
}

fn install(
    host: &mut Host,
    verified: &VerifiedPackage,
    granted: &[Capability],
) -> (extension_host::InstallationId, extension_host::Session) {
    let installation = host
        .register(verified.manifest().clone(), verified.digest())
        .unwrap();
    host.replace_grants(
        installation,
        granted
            .iter()
            .map(|capability| Grant {
                capability: *capability,
                resource: Resource::Vault {
                    vault_id: "vault".into(),
                },
            })
            .collect(),
    )
    .unwrap();
    host.enable(installation).unwrap();
    let session = host
        .open_session_for_api(installation, HOST_API_V0_5)
        .unwrap();
    (installation, session)
}

#[test]
fn an_ordinary_signed_vocab_package_reads_and_schedules_real_core() {
    let (_scratch, app, _book_id) = setup_core();
    let (verified, package_bytes) = signed_package();

    assert_eq!(verified.manifest().package_id, "org.qusor.vocab");
    assert_eq!(
        verified.manifest().required_capabilities,
        vec![Capability::VocabRead, Capability::VocabReview]
    );
    assert_eq!(
        verified.manifest().contributions[0].kind,
        ContributionKind::VocabWorkspace
    );
    assert_eq!(verified.digest(), sha256(&package_bytes));

    let mut host = Host::new("vault", [Capability::VocabRead, Capability::VocabReview]).unwrap();
    let (_installation, session) = install(
        &mut host,
        &verified,
        &[Capability::VocabRead, Capability::VocabReview],
    );
    assert!(host.contributions().iter().any(|(_, contribution)| {
        contribution.kind == ContributionKind::VocabWorkspace
            && contribution.id == "org.qusor.vocab.workspace"
    }));
    let mut bridge = VocabBridge::new(app, "vault", NOW, NOW - DAY);

    let seen = RefCell::new(Vec::new());
    let result = run_managed_raw(verified.module_bytes(), |bytes| {
        let request =
            VocabRequest::from_slice(bytes).map_err(|_| RuntimeError::new("invalid_request"))?;
        seen.borrow_mut().push(request.operation.clone());
        let permit = host
            .authorize_vocab(&session, &request)
            .map_err(|_| RuntimeError::new("permission_denied"))?;
        let response = host
            .dispatch(permit, |context| {
                assert_eq!(context.vault_id, "vault");
                let expected = match &request.operation {
                    VocabOperation::Review { .. } => Capability::VocabReview,
                    VocabOperation::Decks {} | VocabOperation::Queue { .. } => {
                        Capability::VocabRead
                    }
                };
                assert_eq!(context.request.capability, expected);
                Ok(bridge.respond(&request))
            })
            .map_err(|_| RuntimeError::new("dispatch_failed"))?;
        response
            .to_json()
            .map(String::into_bytes)
            .map_err(|_| RuntimeError::new("invalid_response"))
    })
    .unwrap();

    assert_eq!(result.calls, 3);
    assert!(result.fuel_consumed > 0);
    assert_eq!(
        seen.borrow().as_slice(),
        &[
            VocabOperation::Decks {},
            VocabOperation::Queue {
                wordbook_id: 1,
                limit: 3
            },
            VocabOperation::Review {
                card_id: 1,
                rating: VocabRating::Good
            }
        ]
    );
    let response = VocabResponse::from_json(&result.output).unwrap();
    let VocabResult::Review(outcome) = &response.result else {
        panic!("expected a committed Core review: {response:?}");
    };
    assert_eq!(outcome.card_id, 1);
    assert_eq!(outcome.stage, VocabStage::Review);
    assert_eq!(outcome.scheduled_minutes, 3 * 24 * 60);
    assert_eq!(outcome.due_at_ms, NOW + 3 * DAY);

    let archive = bridge.app().export_study().unwrap();
    assert_eq!(archive.reviews.len(), 1);
    assert_eq!(archive.reviews[0].stage_before, StudyStage::New);
    assert_eq!(archive.reviews[0].rating, StudyRating::Good);
    assert_eq!(archive.schedules.len(), 1);
    assert_eq!(archive.schedules[0].card_id, 1);
    assert_eq!(archive.schedules[0].due_at_ms, NOW + 3 * DAY);
}

#[test]
fn a_read_only_package_cannot_schedule_reviews_through_the_host() {
    let (_scratch, app, _book_id) = setup_core();
    let verified = read_only_package();
    let mut host = Host::new("vault", [Capability::VocabRead, Capability::VocabReview]).unwrap();
    let (_installation, session) = install(&mut host, &verified, &[Capability::VocabRead]);
    let mut bridge = VocabBridge::new(app, "vault", NOW, NOW - DAY);

    let result = run_managed_raw(verified.module_bytes(), |bytes| {
        let request =
            VocabRequest::from_slice(bytes).map_err(|_| RuntimeError::new("invalid_request"))?;
        let permit = host
            .authorize_vocab(&session, &request)
            .map_err(|_| RuntimeError::new("permission_denied"))?;
        let response = host
            .dispatch(permit, |_| Ok(bridge.respond(&request)))
            .map_err(|_| RuntimeError::new("dispatch_failed"))?;
        response
            .to_json()
            .map(String::into_bytes)
            .map_err(|_| RuntimeError::new("invalid_response"))
    });
    assert!(result.is_err());
    assert!(bridge.app().export_study().unwrap().reviews.is_empty());
}

#[test]
fn another_key_or_tampered_package_bytes_are_rejected() {
    let (_, package_bytes) = signed_package();
    assert!(
        verify_for_host(
            &package_bytes,
            &publisher_key(&[0x44; 32]),
            &SUPPORTED_HOST_APIS
        )
        .is_err()
    );
    let mut tampered = package_bytes.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert!(verify_for_host(&tampered, &publisher_key(&SEED), &SUPPORTED_HOST_APIS).is_err());
}
