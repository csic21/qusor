//! Manual experiment CLI. Probe grants apply only to the built-in synthetic fixture.
use qusor_package_probe::{
    package::verify,
    store::{GrantScope, Result, Store, read_package},
};
use qusor_wasm_probe::{Options, Package, Probe};
use serde_json::{Value, json};
use std::{env, path::Path, process};

fn main() {
    match env::args().nth(1).as_deref() {
        Some("--managed-worker") => match qusor_package_probe::supervisor::worker_main() {
            Ok(true) => return,
            Ok(false) => process::exit(1),
            Err(error) => {
                eprintln!("managed worker: {error}");
                process::exit(1);
            }
        },
        Some("supervise") => {
            let args: Vec<_> = env::args().collect();
            if args.len() != 3 {
                eprintln!("usage: supervise STORE");
                process::exit(1);
            }
            if let Err(error) = qusor_package_probe::supervisor::run_cli(Path::new(&args[2])) {
                eprintln!("supervisor: {error}");
                process::exit(1);
            }
            return;
        }
        _ => {}
    }

    match run() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            println!("{}", json!({"ok": false, "error": error.to_string()}));
            process::exit(1);
        }
    }
}
fn run() -> Result<Value> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        return Err("command required".into());
    };
    match command {
        "verify" | "probe" if args.len() == 3 => {
            let bytes = read_package(Path::new(&args[1]))?;
            let package = verify(&bytes, &args[2])?;
            if command == "verify" {
                return Ok(json!({"ok":true,"digest":package.digest(),
                    "package_id":package.manifest().package_id,"publisher_key":package.publisher_key(),
                    "publisher_fingerprint":package.publisher_fingerprint(),"profile":package.profile()}));
            }
            let runtime = Package::from_host_bytes(package.manifest().clone(),
                package.digest().to_owned(), package.module_bytes().to_vec())?;
            let result = Probe::new(runtime, Options::default())?.run()?;
            Ok(json!({"ok":true,"code":"ok","result":result}))
        }
        "private-status" if args.len() == 3 => {
            Ok(json!({"ok":true,"private":Store::open(Path::new(&args[1]))?.private_status(&args[2])?}))
        }
        "initialize-private" if args.len() == 6 || args.len() == 8 => {
            let input = read_package(Path::new(&args[5]))?;
            let crash = crash_option(&args,6)?;
            let receipt = Store::open(Path::new(&args[1]))?.initialize_private(&args[2],args[3].parse()?,&args[4],&input,crash)?;
            Ok(json!({"ok":true,"receipt":receipt}))
        }
        "upgrade-private" if args.len() == 6 || args.len() == 8 => {
            let bytes = read_package(Path::new(&args[4]))?;
            let package = verify(&bytes,&args[5])?;
            let crash = crash_option(&args,6)?;
            let receipt = Store::open(Path::new(&args[1]))?.upgrade_private(&args[2],args[3].parse()?,&package,crash)?;
            Ok(json!({"ok":true,"receipt":receipt}))
        }
        "rollback-private" if args.len() == 6 || args.len() == 8 => {
            let crash = crash_option(&args,6)?;
            let receipt = Store::open(Path::new(&args[1]))?.rollback_private(&args[2],args[3].parse()?,&args[4],args[5].parse()?,crash)?;
            Ok(json!({"ok":true,"receipt":receipt}))
        }
        "grant" if args.len() == 9 || args.len() == 11 => {
            let crash = crash_option(&args, 9)?;
            let scope = GrantScope { capability: extension_api::Capability::DocumentMetadataRead, library: args[5].clone(), document: args[6].clone(), version: args[7].clone(), expires_at: args[8].parse()? };
            let receipt = Store::open(Path::new(&args[1]))?.grant(&args[2], args[3].parse()?, &args[4], scope, crash)?;
            Ok(json!({"ok":true,"receipt":receipt}))
        }
        "disable" | "revoke" if args.len() == 5 || args.len() == 7 => {
            let crash = crash_option(&args, 5)?;
            let receipt = Store::open(Path::new(&args[1]))?.stop(&args[2], args[3].parse()?, &args[4], command, crash)?;
            Ok(json!({"ok":true,"receipt":receipt}))
        }
        "resume" if args.len() == 6 => {
            let session = Store::open(Path::new(&args[1]))?.resume(&args[2], &args[3], &args[4], &args[5])?;
            Ok(json!({"ok":true,"session":session,"scope":"policy-only"}))
        }
        "execute" if args.len() == 6 => {
            let mut store = Store::open(Path::new(&args[1]))?;
            let session = store.resume(&args[2], &args[3], &args[4], &args[5])?;
            let result = store.execute(&session, &args[3], &args[4], &args[5])?;
            Ok(json!({"ok":true,"result":result,"scope":"synthetic-metadata-execution"}))
        }
        "check-session" if args.len() == 6 => {
            let token: String = serde_json::from_str(&args[2])?;
            Store::open(Path::new(&args[1]))?.check_session(&token, &args[3], &args[4], &args[5])?;
            Ok(json!({"ok":true}))
        }
        "status" if args.len() == 2 => {
            let store=Store::open(Path::new(&args[1]))?;
            Ok(json!({"ok":true,"installations":store.status()?,"worker_leases":store.worker_leases()?}))
        }
        "install" if args.len() == 6 || args.len() == 8 => {
            let bytes = read_package(Path::new(&args[4]))?;
            let package = verify(&bytes, &args[5])?;
            let crash = crash_option(&args, 6)?;
            let receipt = Store::open(Path::new(&args[1]))?.install(&args[2], args[3].parse()?, &package, crash)?;
            Ok(json!({"ok":true,"receipt":receipt}))
        }
        "uninstall" if args.len() == 5 || args.len() == 7 => {
            let crash = crash_option(&args, 5)?;
            let receipt = Store::open(Path::new(&args[1]))?.uninstall(&args[2], args[3].parse()?, &args[4], crash)?;
            Ok(json!({"ok":true,"receipt":receipt}))
        }
        _ => Err("usage: verify|probe PACKAGE KEY; status STORE; install STORE OP REV PACKAGE KEY [--crash POINT]; uninstall|disable|revoke STORE OP REV PACKAGE_ID [--crash POINT]; grant STORE OP REV PACKAGE_ID LIBRARY DOCUMENT VERSION EXPIRES [--crash POINT]; resume|execute STORE PACKAGE_ID LIBRARY DOCUMENT VERSION; check-session STORE SESSION_JSON LIBRARY DOCUMENT VERSION".into()),
    }
}
fn crash_option(args: &[String], start: usize) -> Result<Option<&str>> {
    if args.len() == start {
        return Ok(None);
    }
    if args.get(start).map(String::as_str) != Some("--crash") {
        return Err("unknown option".into());
    }
    Ok(args.get(start + 1).map(String::as_str))
}
