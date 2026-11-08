use qusor_capture_probe::{
    core::Core,
    host::CaptureHost,
    pdf_compound::PdfCompoundDraft,
    pdf_contract::{PdfDraft, PdfSnapshot},
    pdf_host::PdfCaptureHost,
};
use qusor_package_probe::{
    package::verify,
    store::{Store, read_package},
};
use serde_json::{Value, json};
use std::{env, io::Read, os::unix::fs::OpenOptionsExt, path::Path};
fn read_bounded(path: &str, limit: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > limit {
        return Err("input must be a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("input budget exceeded".into());
    }
    Ok(bytes)
}
fn run() -> Result<Value, Box<dyn std::error::Error>> {
    let a: Vec<String> = env::args().skip(1).collect();
    let arg = |i: usize| a.get(i).map(String::as_str).ok_or("missing argument");
    match arg(0)? {
        "pdf-import" if a.len() == 7 || (a.len() == 9 && arg(7)? == "--crash") => {
            let bytes = read_bounded(arg(5)?, 16 * 1024 * 1024)?;
            let snapshot = PdfSnapshot::from_slice(&read_bounded(arg(6)?, 32 * 1024 * 1024)?)?;
            let receipt = Core::open(arg(1)?)?.import_document(
                arg(3)?,
                arg(2)?,
                arg(4)?,
                &bytes,
                &snapshot,
                if a.len() == 9 { Some(arg(8)?) } else { None },
            )?;
            Ok(json!({"ok":true,"receipt":receipt}))
        }
        "pdf-receipt" if a.len() == 3 => {
            Ok(json!({"ok":true,"receipt":Core::open(arg(1)?)?.pdf_receipt(arg(2)?)?}))
        }
        "document-receipt" if a.len() == 3 => {
            Ok(json!({"ok":true,"receipt":Core::open(arg(1)?)?.document_receipt(arg(2)?)?}))
        }
        "operation" if a.len() == 3 => {
            Ok(json!({"ok":true,"operation":Core::open(arg(1)?)?.query_operation(arg(2)?)?}))
        }
        "pdf-original" if a.len() == 4 => {
            use std::io::Write;
            let bytes = Core::open(arg(1)?)?.read_original(arg(2)?.parse()?)?;
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(arg(3)?)?;
            output.write_all(&bytes)?;
            output.sync_all()?;
            Ok(json!({"ok":true,"bytes":bytes.len()}))
        }
        "pdf-document" if a.len() == 3 => {
            Ok(json!({"ok":true,"document":Core::open(arg(1)?)?.query_document(arg(2)?.parse()?)?}))
        }
        "pdf-query" if a.len() == 3 => {
            Ok(json!({"ok":true,"capture":Core::open(arg(1)?)?.query_pdf(arg(2)?.parse()?)?}))
        }
        "pdf-compound-query" if a.len() == 3 => Ok(
            json!({"ok":true,"capture":Core::open(arg(1)?)?.query_pdf_complete(arg(2)?.parse()?)?}),
        ),
        "pdf-resolve" if a.len() == 4 => {
            let draft = PdfDraft::from_slice(&read_bounded(arg(3)?, 4096)?)?;
            Ok(
                json!({"ok":true,"selection":Core::open(arg(1)?)?.resolve_pdf(arg(2)?.parse()?,&draft)?}),
            )
        }
        "pdf-compound-resolve" if a.len() == 4 => {
            let draft = PdfCompoundDraft::from_slice(&read_bounded(arg(3)?, 4096)?)?;
            Ok(
                json!({"ok":true,"selection":Core::open(arg(1)?)?.resolve_pdf_compound(arg(2)?.parse()?,&draft)?}),
            )
        }
        "pdf-capture" | "pdf-compound-capture"
            if a.len() == 9
                || (a.len() == 11 && arg(9)? == "--accept")
                || (a.len() == 13 && arg(9)? == "--accept" && arg(11)? == "--crash") =>
        {
            let bytes = read_bounded(arg(7)?, 4096)?;
            let host = if arg(0)? == "pdf-compound-capture" {
                PdfCaptureHost::open_compound(
                    Path::new(arg(1)?),
                    Path::new(arg(2)?),
                    arg(3)?,
                    arg(5)?,
                    arg(6)?.parse()?,
                    PdfCompoundDraft::from_slice(&bytes)?,
                )?
            } else {
                PdfCaptureHost::open(
                    Path::new(arg(1)?),
                    Path::new(arg(2)?),
                    arg(3)?,
                    arg(5)?,
                    arg(6)?.parse()?,
                    PdfDraft::from_slice(&bytes)?,
                )?
            };
            host.grant()?;
            let proposal = host.propose()?;
            let preview = if let Some(compound) = proposal.compound() {
                json!({"draft":compound,"draft_hash":proposal.hash(),"provenance":host.provenance(),
                    "exact":compound.exact,"segment_count":compound.segments.len(),
                    "page_numbers":compound.segments.iter().map(|part|part.page_index+1).collect::<Vec<_>>(),
                    "title":host.document_title()?,"user_note":arg(8)?})
            } else {
                json!({"draft":proposal.draft()?,"draft_hash":proposal.hash(),"provenance":host.provenance(),
                "title":host.document_title()?,"page_number":proposal.draft()?.page_index+1,"user_note":arg(8)?})
            };
            if a.len() == 9 {
                return Ok(json!({"ok":true,"committed":false,"preview":preview}));
            }
            let approval = host.approve(proposal, arg(10)?, arg(4)?, arg(8)?)?;
            let receipt = host
                .accept_with_crash(approval, if a.len() == 13 { Some(arg(12)?) } else { None })?;
            Ok(json!({"ok":true,"receipt":receipt,"preview":preview}))
        }
        "export-all" if a.len() == 2 => {
            Ok(json!({"ok":true,"archive":Core::open(arg(1)?)?.export_all()?}))
        }
        "restore-all" if a.len() == 3 || (a.len() == 5 && arg(3)? == "--crash") => {
            let bytes = read_bounded(arg(2)?, 64 * 1024 * 1024)?;
            Ok(
                json!({"ok":true,"receipt":Core::open(arg(1)?)?.restore_all_json(&bytes,if a.len()==5{Some(arg(4)?)}else{None})?}),
            )
        }
        "install" if a.len() == 6 => {
            let package = verify(&read_package(Path::new(arg(4)?))?, arg(5)?)?;
            let receipt = Store::open(Path::new(arg(1)?))?.install(
                arg(2)?,
                arg(3)?.parse()?,
                &package,
                None,
            )?;
            Ok(json!({"ok":true,"receipt":receipt}))
        }
        "capture" => {
            let host = CaptureHost::open(Path::new(arg(1)?), Path::new(arg(2)?), arg(3)?, arg(5)?)?;
            host.grant()?;
            let p = host.propose()?;
            let preview =
                json!({"draft":p.draft(),"draft_hash":p.hash(),"provenance":host.provenance()});
            if (a.len() == 9 || (a.len() == 11 && arg(9)? == "--crash")) && arg(7)? == "--accept" {
                let approval = host.approve(p, arg(8)?, arg(4)?, arg(6)?)?;
                let crash = if a.len() == 11 && arg(9)? == "--crash" {
                    Some(arg(10)?)
                } else {
                    None
                };
                let receipt = host.accept_with_crash(approval, crash)?;
                Ok(json!({"ok":true,"receipt":receipt,"preview":preview}))
            } else if a.len() == 7 {
                Ok(json!({"ok":true,"preview":preview,"committed":false}))
            } else {
                Err("capture PACKAGES CORE VAULT OP PACKAGE_ID USER_NOTE [--accept EXPECTED_DRAFT_HASH]".into())
            }
        }
        "query" if a.len() == 3 => {
            Ok(json!({"ok":true,"capture":Core::open(arg(1)?)?.query(arg(2)?.parse()?)?}))
        }
        "receipt" if a.len() == 3 => {
            Ok(json!({"ok":true,"receipt":Core::open(arg(1)?)?.receipt(arg(2)?)?}))
        }
        "resolve" if a.len() == 6 => Ok(
            json!({"ok":true,"selection":Core::open(arg(1)?)?.resolve(arg(2)?.parse()?,arg(3)?,arg(4)?,arg(5)?)?}),
        ),
        "resolve-version" if a.len() == 7 => Ok(
            json!({"ok":true,"selection":Core::open(arg(1)?)?.resolve_version(arg(2)?.parse()?,arg(3)?,arg(4)?,arg(5)?,arg(6)?)?}),
        ),
        "export" if a.len() == 2 => Ok(json!({"ok":true,"bundle":Core::open(arg(1)?)?.export()?})),
        "restore" if a.len() == 3 || (a.len() == 5 && arg(3)? == "--crash") => {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(arg(2)?)?;
            if !file.metadata()?.is_file() {
                return Err("restore requires regular file".into());
            }
            let mut bytes = Vec::new();
            file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 64 * 1024 * 1024 {
                return Err("restore budget exceeded".into());
            }
            Ok(
                json!({"ok":true,"receipt":Core::open(arg(1)?)?.restore_json(&bytes,if a.len()==5{Some(arg(4)?)}else{None})?}),
            )
        }
        _ => Err("unknown command".into()),
    }
}
fn main() {
    match run() {
        Ok(v) => println!("{v}"),
        Err(e) => {
            println!("{}", json!({"ok":false,"error":e.to_string()}));
            std::process::exit(1)
        }
    }
}
