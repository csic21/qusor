//! Independent API0.3 text-provider author. Uses only the exported public SDK.
use extension_api::{
    ApiRange, Capability, ContributionKind, HOST_API_V0_3, Manifest, composition::*,
};
use qusor_package_probe::package::{PackageFile, build_signed_for_host, publisher_key};
use std::{env, error::Error, fs, process::ExitCode};
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("component_author: {e}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err(
            "usage: component_author <manifest.json> <seed-file-64hex> <output.qpk>".into(),
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
        view: NativeEditorView::TextEditor {
            label: "Thought".into(),
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
    let module = wat::parse_str(include_str!("../component_text.wat"))?;
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
