//! Manual experiment CLI. Probe grants apply only to the built-in synthetic fixture.
use qusor_package_probe::{
    package::verify,
    store::{Result, Store, read_package},
};
use qusor_wasm_probe::{Options, Package, Probe};
use serde_json::{Value, json};
use std::{env, path::Path, process};

fn main() {
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
        "status" if args.len() == 2 => {
            Ok(json!({"ok":true,"installations":Store::open(Path::new(&args[1]))?.status()?}))
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
        _ => Err("usage: verify|probe PACKAGE KEY; status STORE; install STORE OP REV PACKAGE KEY [--crash POINT]; uninstall STORE OP REV PACKAGE_ID [--crash POINT]".into()),
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
