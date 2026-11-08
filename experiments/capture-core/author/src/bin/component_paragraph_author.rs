//! Independent API0.3 paragraph-provider author. Uses only the exported public SDK.
use extension_api::{
    ApiRange, Capability, ContributionKind, HOST_API_V0_3, Manifest, composition::*,
};
use qusor_package_probe::package::{PackageFile, build_signed_for_host, publisher_key};
use std::{env, error::Error, fs, process::ExitCode};
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("component_paragraph_author: {e}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err(
            "usage: component_paragraph_author <manifest.json> <seed-file-64hex> <output.qpk>"
                .into(),
        );
    }
    let manifest = Manifest::from_slice(&fs::read(&args[0])?)?;
    if manifest.host_api
        != (ApiRange {
            major: 0,
            min_minor: 3,
            max_minor: 3,
        })
        || !manifest.supports_api(HOST_API_V0_3)
    {
        return Err("provider requires explicit API0.3 only".into());
    }
    let contributions: Vec<_> = manifest
        .contributions
        .iter()
        .filter(|c| {
            c.kind == ContributionKind::ComponentProvider
                && c.required_capabilities.contains(&Capability::NoteDraftEdit)
        })
        .collect();
    if contributions.len() != 1 {
        return Err("exactly one component provider required".into());
    }
    let descriptor = CompositionDescriptor {
        composition_protocol: 1,
        api_version: HOST_API_V0_3,
        contribution_id: contributions[0].id.clone(),
        role: ComponentRole::AnnotationPlainTextEditor,
        contract: ANNOTATION_EDITOR_CONTRACT.into(),
        view: NativeEditorView::ParagraphEditor {
            protocols: None,
            label: "Paragraphs".into(),
            placeholder: "Write your thought".into(),
        },
    };
    descriptor.validate_for_manifest(&manifest)?;
    let seed_text = fs::read_to_string(&args[1])?;
    let seed_text = seed_text.trim();
    if seed_text.len() != 64 || !seed_text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("seed must be 32 hex bytes".into());
    }
    let mut seed = [0u8; 32];
    for (i, b) in seed.iter_mut().enumerate() {
        *b = u8::from_str_radix(&seed_text[i * 2..i * 2 + 2], 16)?;
    }
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Err("paragraph author currently qualified only on Linux x86_64".into());
    }
    let clang_version = std::process::Command::new("clang")
        .arg("--version")
        .output()?;
    let rust_version = std::process::Command::new("rustc")
        .arg("--version")
        .output()?;
    if !clang_version.status.success()
        || !String::from_utf8_lossy(&clang_version.stdout).contains("clang version 21.1.8")
        || !rust_version.status.success()
        || !String::from_utf8_lossy(&rust_version.stdout).starts_with("rustc 1.97.1 ")
    {
        return Err("qualification requires clang21.1.8 and rust1.97.1 linker".into());
    }
    let temp = env::temp_dir().join(format!("qusor-paragraph-author-{}", std::process::id()));
    fs::create_dir(&temp)?;
    let build = (|| -> Result<Vec<u8>, Box<dyn Error>> {
        fs::write(
            temp.join("guest.c"),
            include_str!("../component_paragraph.c"),
        )?;
        fs::write(
            temp.join("entry.s"),
            include_str!("../component_paragraph_entry.s"),
        )?;
        let run = |tool: &str, args: &[&str]| -> Result<(), Box<dyn Error>> {
            let status = std::process::Command::new(tool)
                .args(args)
                .current_dir(&temp)
                .status()?;
            if !status.success() {
                return Err(format!("guest compiler failed: {tool}").into());
            }
            Ok(())
        };
        run(
            "clang",
            &[
                "--target=wasm32-unknown-unknown",
                "-O2",
                "-ffreestanding",
                "-fno-builtin",
                "-nostdlib",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-c",
                "guest.c",
                "-o",
                "guest.o",
            ],
        )?;
        run(
            "clang",
            &[
                "--target=wasm32-unknown-unknown",
                "-mmultivalue",
                "-c",
                "entry.s",
                "-o",
                "entry.o",
            ],
        )?;
        let sysroot = std::process::Command::new("rustc")
            .args(["--print", "sysroot"])
            .output()?;
        if !sysroot.status.success() {
            return Err("cannot resolve toolchain linker".into());
        }
        let linker = std::path::Path::new(std::str::from_utf8(&sysroot.stdout)?.trim())
            .join("lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld/wasm-ld");
        run(
            linker.to_str().ok_or("linker path must be UTF-8")?,
            &[
                "--no-entry",
                "--export=run",
                "--export-memory",
                "--initial-memory=131072",
                "--max-memory=131072",
                "guest.o",
                "entry.o",
                "-o",
                "module.wasm",
            ],
        )?;
        Ok(fs::read(temp.join("module.wasm"))?)
    })();
    fs::remove_dir_all(&temp)?;
    let module = build?;
    if module.len() > 256 * 1024 {
        return Err("module exceeds frozen execution budget".into());
    }
    let package = build_signed_for_host(
        manifest,
        "module.wasm",
        vec![
            PackageFile {
                path: "module.wasm".into(),
                mime: "application/wasm".into(),
                bytes: module,
            },
            PackageFile {
                path: "assets/ui-composition.json".into(),
                mime: "application/json".into(),
                bytes: descriptor.to_json()?.into_bytes(),
            },
        ],
        &seed,
        &[HOST_API_V0_3],
    )?;
    fs::write(&args[2], package)?;
    println!("{}", publisher_key(&seed));
    Ok(())
}
