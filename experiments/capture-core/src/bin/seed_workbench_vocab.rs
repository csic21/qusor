//! Seed a local Core database with the workbench vocabulary fixture.
//! Trusted experiment input: two synthetic wordbooks, no licensed word list.
use core_api::vocab::{NewWord, WordbookSeed};
use learning_application::study::StudyApplication;
use serde_json::json;
use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};
use storage_sqlite::SqliteStore;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| i64::try_from(value.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(1)
}

fn books() -> Vec<WordbookSeed> {
    vec![
        WordbookSeed {
            title: "CET-4 示例".into(),
            description: "合成示例词书，不是授权词表。".into(),
            words: vec![
                NewWord {
                    term: "abandon".into(),
                    phonetic: String::new(),
                    pos: String::new(),
                    meaning: "放弃；抛弃".into(),
                    example: "Do not abandon the plan.".into(),
                },
                NewWord {
                    term: "benefit".into(),
                    phonetic: String::new(),
                    pos: String::new(),
                    meaning: "益处；使受益".into(),
                    example: "Reading has many benefits.".into(),
                },
                NewWord {
                    term: "candidate".into(),
                    phonetic: String::new(),
                    pos: String::new(),
                    meaning: "候选人；申请者".into(),
                    example: "The candidate answered well.".into(),
                },
            ],
        },
        WordbookSeed {
            title: "QUSOR 术语".into(),
            description: "合成示例词书，用于展示第二本词书。".into(),
            words: vec![
                NewWord {
                    term: "plugin".into(),
                    phonetic: String::new(),
                    pos: String::new(),
                    meaning: "插件；通过公开能力接入的功能包".into(),
                    example: "The plugin only submits bounded requests.".into(),
                },
                NewWord {
                    term: "vault".into(),
                    phonetic: String::new(),
                    pos: String::new(),
                    meaning: "本地库；Core 的绑定范围".into(),
                    example: "The Host binds the vault for the guest.".into(),
                },
            ],
        },
    ]
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let data = arguments
        .next()
        .ok_or("usage: seed_workbench_vocab DATA_DIR RECEIPT")?;
    let receipt_path = arguments.next().ok_or("receipt path required")?;
    let data = Path::new(&data);
    fs::create_dir_all(data).map_err(|error| error.to_string())?;
    fs::set_permissions(data, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let mut app =
        StudyApplication::new(SqliteStore::open(data).map_err(|error| error.to_string())?);
    let now = now_ms();
    let mut receipt_books = Vec::new();
    for seed in books() {
        let book = app
            .seed_wordbook("workbench-vault", &seed, now)
            .map_err(|error| error.to_string())?;
        receipt_books.push(json!({
            "wordbook_id": book.id,
            "title": book.title,
            "cards": seed.words.len(),
        }));
    }
    let decks = app
        .decks("workbench-vault", now)
        .map_err(|error| error.to_string())?;
    let receipt = json!({
        "vault": "workbench-vault",
        "wordbooks": receipt_books,
        "decks": decks.iter().map(|deck| json!({
            "wordbook_id": deck.wordbook.id,
            "title": deck.wordbook.title,
            "total": deck.total,
            "due": deck.due,
            "new": deck.new,
        })).collect::<Vec<_>>(),
    });
    fs::write(
        &receipt_path,
        serde_json::to_string_pretty(&receipt).unwrap(),
    )
    .map_err(|error| error.to_string())?;
    println!("{receipt}");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("seed_workbench_vocab: {error}");
            ExitCode::FAILURE
        }
    }
}
