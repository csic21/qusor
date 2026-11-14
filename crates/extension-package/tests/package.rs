use extension_api::{HOST_API, Manifest};
use extension_package::package::{
    DevUnsignedOptions, EXECUTION_PROFILE, HARD_MAX_PACKAGE_BYTES, MAX_FILE_BYTES, MAX_FILES,
    MAX_PACKAGE_BYTES, MAX_PATH_BYTES, MAX_PATH_DEPTH, PackageBudgets, PackageErrorCode,
    PackageFile, build_signed, build_signed_with_budgets, dev_unsigned_supported, publisher_key,
    verify, verify_with_budgets,
};
use sha2::{Digest, Sha256};

const SEED: [u8; 32] = [7; 32];
const WASM: &[u8] = b"\0asm\x01\0\0\0";

fn manifest() -> Manifest {
    Manifest::from_json(
        r#"{"manifest_version":1,"package_id":"org.example.reader","publisher_id":"org.example","package_version":{"major":1,"minor":0,"patch":0},"host_api":{"major":0,"min_minor":1,"max_minor":1},"required_capabilities":["selection.read"],"optional_capabilities":[],"contributions":[{"id":"org.example.reader.inspect","title":"Inspect","kind":"selection_action","required_capabilities":["selection.read"]}]}"#,
    ).unwrap()
}

fn module(path: &str) -> PackageFile {
    PackageFile {
        path: path.to_owned(),
        mime: "application/wasm".to_owned(),
        bytes: WASM.to_vec(),
    }
}

fn asset(path: &str) -> PackageFile {
    PackageFile {
        path: path.to_owned(),
        mime: "text/plain".to_owned(),
        bytes: b"immutable asset".to_vec(),
    }
}

fn fixture() -> Vec<u8> {
    build_signed(
        manifest(),
        "module.wasm",
        vec![module("module.wasm"), asset("assets/notice.txt")],
        &SEED,
    )
    .unwrap()
}

fn replaced(bytes: &[u8], old: &str, new: &str) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).unwrap();
    assert!(
        text.contains(old),
        "fixture does not contain replacement target"
    );
    text.replacen(old, new, 1).into_bytes()
}

fn rejected(bytes: &[u8], code: PackageErrorCode) {
    assert_eq!(verify(bytes, &publisher_key(&SEED)).unwrap_err().code, code);
}

#[test]
fn deterministic_package_binds_frozen_bytes_and_all_files() {
    let bytes = fixture();
    let reversed = build_signed(
        manifest(),
        "module.wasm",
        vec![asset("assets/notice.txt"), module("module.wasm")],
        &SEED,
    )
    .unwrap();
    assert_eq!(bytes, reversed);
    let verified = verify(&bytes, &publisher_key(&SEED)).unwrap();
    assert_eq!(verified.raw_bytes(), bytes);
    assert_eq!(verified.manifest(), &manifest());
    assert_eq!(verified.module_bytes(), WASM);
    assert_eq!(
        verified.file("assets/notice.txt"),
        Some(("text/plain", b"immutable asset".as_slice()))
    );
    assert_eq!(verified.file("../module.wasm"), None);
    assert_eq!(verified.entrypoint(), "module.wasm");
    assert_eq!(verified.profile(), EXECUTION_PROFILE);
    assert_eq!(verified.verifier_policy(), 1);
    assert_eq!(verified.digest(), format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(
        verified.publisher_key(),
        "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
    );
    assert_eq!(
        verified.digest(),
        "e148f545dcfdfb238981ed863daea3df518cbd9b481c8a967b982ae308050165"
    );
}

#[test]
fn manifest_identity_permissions_and_file_metadata_are_signed() {
    let bytes = fixture();
    for (old, new) in [
        (
            "\"publisher_id\":\"org.example\"",
            "\"publisher_id\":\"org.other\"",
        ),
        ("\"patch\":0", "\"patch\":1"),
        ("\"title\":\"Inspect\"", "\"title\":\"Changed\""),
        (
            "\"optional_capabilities\":[]",
            "\"optional_capabilities\":[\"capture.propose\"]",
        ),
        ("\"mime\":\"text/plain\"", "\"mime\":\"text/csv\""),
        (
            "\"path\":\"assets/notice.txt\"",
            "\"path\":\"assets/renamed.txt\"",
        ),
    ] {
        rejected(
            &replaced(&bytes, old, new),
            PackageErrorCode::InvalidSignature,
        );
    }
    // Change the package ID and contribution namespace coherently.
    let bytes = String::from_utf8(bytes)
        .unwrap()
        .replace("org.example.reader", "org.example.viewer");
    rejected(bytes.as_bytes(), PackageErrorCode::InvalidSignature);
}

#[test]
fn module_asset_bytes_and_rehashed_tampering_are_rejected() {
    let bytes = fixture();
    rejected(
        &replaced(
            &bytes,
            "696d6d757461626c65206173736574",
            "696d6d757461626c65206173736575",
        ),
        PackageErrorCode::FileMismatch,
    );
    rejected(
        &replaced(&bytes, "0061736d01000000", "0061736d01000001"),
        PackageErrorCode::FileMismatch,
    );
    let asset_hash = format!("{:x}", Sha256::digest(b"immutable asset"));
    let modified_hash = format!("{:x}", Sha256::digest(b"immutable asseu"));
    let changed = replaced(
        &bytes,
        "696d6d757461626c65206173736574",
        "696d6d757461626c65206173736575",
    );
    rejected(
        &replaced(&changed, &asset_hash, &modified_hash),
        PackageErrorCode::InvalidSignature,
    );
}

#[test]
fn host_trust_is_required_even_for_a_valid_other_key_signature() {
    let package = fixture();
    assert_eq!(
        verify(&package, &publisher_key(&[8; 32])).unwrap_err().code,
        PackageErrorCode::UntrustedKey
    );
    let attacker = build_signed(
        manifest(),
        "module.wasm",
        vec![module("module.wasm")],
        &[8; 32],
    )
    .unwrap();
    rejected(&attacker, PackageErrorCode::UntrustedKey);
    let altered = replaced(&package, &publisher_key(&SEED), &publisher_key(&[8; 32]));
    assert!(verify(&altered, &publisher_key(&[8; 32])).is_err());
}

#[test]
fn invalid_signature_and_fingerprint_are_rejected() {
    let bytes = fixture();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let signature = value["signature"]["signature"].as_str().unwrap();
    rejected(
        &replaced(&bytes, signature, &"00".repeat(64)),
        PackageErrorCode::InvalidSignature,
    );
    let fingerprint = value["signature"]["publisher_fingerprint"]
        .as_str()
        .unwrap();
    rejected(
        &replaced(&bytes, fingerprint, &"00".repeat(32)),
        PackageErrorCode::UntrustedKey,
    );
}

#[test]
fn unknown_fields_duplicate_fields_and_capabilities_are_rejected() {
    let bytes = fixture();
    for (old, new) in [
        ("{\"payload\":", "{\"extra\":0,\"payload\":"),
        (
            "\"package_format\":1",
            "\"package_format\":1,\"package_format\":1",
        ),
        (
            "\"manifest_version\":1",
            "\"manifest_version\":1,\"manifest_version\":1",
        ),
        ("\"major\":1", "\"major\":1,\"major\":1"),
        ("\"size\":15", "\"size\":15,\"size\":15"),
        (
            "\"mime\":\"text/plain\"",
            "\"link\":\"outside\",\"mime\":\"text/plain\"",
        ),
        (
            "\"algorithm\":\"ed25519\"",
            "\"algorithm\":\"ed25519\",\"algorithm\":\"ed25519\"",
        ),
        ("\"selection.read\"", "\"filesystem.write\""),
    ] {
        rejected(&replaced(&bytes, old, new), PackageErrorCode::Malformed);
    }
}

#[test]
fn unsupported_profile_format_api_and_contributions_are_rejected() {
    let bytes = fixture();
    for (old, new) in [
        ("\"package_format\":1", "\"package_format\":2"),
        (EXECUTION_PROFILE, "native-process-v1"),
        ("\"algorithm\":\"ed25519\"", "\"algorithm\":\"unsigned\""),
        (
            "\"min_minor\":1,\"max_minor\":1",
            "\"min_minor\":2,\"max_minor\":2",
        ),
    ] {
        rejected(&replaced(&bytes, old, new), PackageErrorCode::Incompatible);
    }
    rejected(
        &replaced(&bytes, "\"selection_action\"", "\"webview\""),
        PackageErrorCode::Malformed,
    );
    rejected(
        &replaced(&bytes, "\"manifest_version\":1", "\"manifest_version\":2"),
        PackageErrorCode::InvalidManifest,
    );
}

#[test]
fn ambiguous_json_and_hex_encodings_are_rejected() {
    let bytes = fixture();
    let mut whitespace = bytes.clone();
    whitespace.push(b'\n');
    rejected(&whitespace, PackageErrorCode::NonCanonical);
    rejected(
        &replaced(&bytes, "org.example", "org.\\u0065xample"),
        PackageErrorCode::NonCanonical,
    );
    rejected(
        &replaced(&bytes, "0061736d01000000", "0061736D01000000"),
        PackageErrorCode::Malformed,
    );
    rejected(
        &replaced(&bytes, "0061736d01000000", "0061736d"),
        PackageErrorCode::FileMismatch,
    );
    let mut trailing = bytes;
    trailing.extend_from_slice(b"{}");
    rejected(&trailing, PackageErrorCode::Malformed);
}

#[test]
fn traversal_absolute_platform_aliases_and_excessive_paths_are_rejected() {
    let invalid_paths = [
        "../bad",
        "/bad",
        "a//bad",
        "./bad",
        "a/../bad",
        "C:/bad",
        "a\\bad",
        "a/./bad",
        "a/",
        "a/b.",
        "CON",
        "aux.txt",
        "a/LPT1.log",
        "a/b c",
        "a/雪",
    ];
    for path in invalid_paths {
        let error = build_signed(
            manifest(),
            "module.wasm",
            vec![asset(path), module("module.wasm")],
            &SEED,
        )
        .unwrap_err();
        assert_eq!(error.code, PackageErrorCode::UnsafePath, "{path}");
    }
    for path in [
        "a".repeat(MAX_PATH_BYTES + 1),
        std::iter::repeat_n("a", MAX_PATH_DEPTH + 1)
            .collect::<Vec<_>>()
            .join("/"),
    ] {
        let error = build_signed(
            manifest(),
            "module.wasm",
            vec![asset(&path), module("module.wasm")],
            &SEED,
        )
        .unwrap_err();
        assert_eq!(error.code, PackageErrorCode::UnsafePath);
    }
    // Exercise the untrusted verifier path as well as author validation.
    rejected(
        &replaced(&fixture(), "assets/notice.txt", "assets/../bad.txt"),
        PackageErrorCode::UnsafePath,
    );
}

#[test]
fn duplicate_case_conflicting_ancestor_and_unsorted_entries_are_rejected() {
    for (first, second) in [("a", "a"), ("a", "A"), ("a", "a/b"), ("a", "A/b")] {
        let error = build_signed(
            manifest(),
            "module.wasm",
            vec![asset(first), asset(second), module("module.wasm")],
            &SEED,
        )
        .unwrap_err();
        assert_eq!(error.code, PackageErrorCode::UnsafePath);
    }
    rejected(
        &replaced(&fixture(), "assets/notice.txt", "z/notice.txt"),
        PackageErrorCode::UnsafePath,
    );
}

#[test]
fn declared_and_actual_size_and_count_budgets_are_enforced() {
    let bytes = vec![b' '; MAX_PACKAGE_BYTES + 1];
    rejected(&bytes, PackageErrorCode::BudgetExceeded);
    let mut large = asset("large.txt");
    large.bytes.resize(MAX_FILE_BYTES + 1, 0);
    assert_eq!(
        build_signed(
            manifest(),
            "module.wasm",
            vec![large, module("module.wasm")],
            &SEED
        )
        .unwrap_err()
        .code,
        PackageErrorCode::BudgetExceeded
    );
    let mut many = (0..MAX_FILES)
        .map(|index| asset(&format!("asset-{index}")))
        .collect::<Vec<_>>();
    many.push(module("module.wasm"));
    assert_eq!(
        build_signed(manifest(), "module.wasm", many, &SEED)
            .unwrap_err()
            .code,
        PackageErrorCode::BudgetExceeded
    );
    let mut total = (0..5)
        .map(|index| {
            let mut file = asset(&format!("asset-{index}"));
            file.bytes.resize(MAX_FILE_BYTES, 0);
            file
        })
        .collect::<Vec<_>>();
    total.push(module("module.wasm"));
    assert_eq!(
        build_signed(manifest(), "module.wasm", total, &SEED)
            .unwrap_err()
            .code,
        PackageErrorCode::BudgetExceeded
    );
    rejected(
        &replaced(&fixture(), "\"size\":15", "\"size\":1048577"),
        PackageErrorCode::BudgetExceeded,
    );
    rejected(
        &replaced(&fixture(), "\"size\":15", "\"size\":14"),
        PackageErrorCode::FileMismatch,
    );
}

#[test]
fn entrypoint_must_resolve_to_a_listed_wasm_file() {
    assert_eq!(
        build_signed(
            manifest(),
            "missing.wasm",
            vec![module("module.wasm")],
            &SEED
        )
        .unwrap_err()
        .code,
        PackageErrorCode::FileMismatch
    );
    assert_eq!(
        build_signed(manifest(), "text.txt", vec![asset("text.txt")], &SEED)
            .unwrap_err()
            .code,
        PackageErrorCode::FileMismatch
    );
    let bytes = build_signed(
        manifest(),
        "module.wasm",
        vec![module("module.wasm"), module("second.wasm")],
        &SEED,
    )
    .unwrap();
    rejected(
        &replaced(
            &bytes,
            "\"entrypoint\":\"module.wasm\"",
            "\"entrypoint\":\"second.wasm\"",
        ),
        PackageErrorCode::InvalidSignature,
    );
    let mut corrupt = module("module.wasm");
    corrupt.bytes[4] = 2;
    assert_eq!(
        build_signed(manifest(), "module.wasm", vec![corrupt], &SEED)
            .unwrap_err()
            .code,
        PackageErrorCode::FileMismatch
    );
}

#[test]
fn verified_content_cannot_change_after_the_input_buffer_is_reused() {
    let mut bytes = fixture();
    let verified = verify(&bytes, &publisher_key(&SEED)).unwrap();
    bytes.fill(0);
    assert_ne!(verified.raw_bytes(), bytes);
    assert_eq!(verified.module_bytes(), WASM);
    assert!(verify(verified.raw_bytes(), &publisher_key(&SEED)).is_ok());
}

#[test]
fn strict_signature_verification_rejects_weak_identity_keys() {
    let bytes = fixture();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let signature = document["signature"]["signature"].as_str().unwrap();
    let fingerprint = document["signature"]["publisher_fingerprint"]
        .as_str()
        .unwrap();
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let identity_key = format!("01{}", "00".repeat(31));
    let identity_signature = format!("01{}", "00".repeat(63));
    let bytes = replaced(&bytes, &publisher_key(&SEED), &identity_key);
    let bytes = replaced(
        &bytes,
        fingerprint,
        &format!("{:x}", Sha256::digest(identity)),
    );
    let bytes = replaced(&bytes, signature, &identity_signature);
    assert_eq!(
        verify(&bytes, &identity_key).unwrap_err().code,
        PackageErrorCode::InvalidSignature
    );
}

#[test]
fn explicit_host_compatibility_does_not_upgrade_legacy_verifier_or_signed_bytes() {
    use extension_api::{HOST_API, HOST_API_V0_2, SUPPORTED_HOST_APIS};
    use extension_package::package::{build_signed_for_host, verify_for_host};
    let old = fixture();
    assert_eq!(
        verify_for_host(&old, &publisher_key(&SEED), &SUPPORTED_HOST_APIS)
            .unwrap()
            .raw_bytes(),
        old
    );
    assert_eq!(
        build_signed_for_host(
            manifest(),
            "module.wasm",
            vec![module("module.wasm"), asset("assets/notice.txt")],
            &SEED,
            &SUPPORTED_HOST_APIS
        )
        .unwrap(),
        old
    );
    let mut reader = manifest();
    reader.host_api.min_minor = 2;
    reader.host_api.max_minor = 2;
    reader
        .required_capabilities
        .push(extension_api::Capability::ReaderSurface);
    reader.contributions[0].kind = extension_api::ContributionKind::ReaderWorkspace;
    reader.contributions[0]
        .required_capabilities
        .push(extension_api::Capability::ReaderSurface);
    assert_eq!(
        build_signed(
            reader.clone(),
            "module.wasm",
            vec![module("module.wasm")],
            &SEED
        )
        .unwrap_err()
        .code,
        PackageErrorCode::Incompatible
    );
    let mut falsely_legacy = reader.clone();
    falsely_legacy.host_api.min_minor = 1;
    assert_eq!(
        build_signed_for_host(
            falsely_legacy,
            "module.wasm",
            vec![module("module.wasm")],
            &SEED,
            &SUPPORTED_HOST_APIS
        )
        .unwrap_err()
        .code,
        PackageErrorCode::InvalidManifest
    );
    let bytes = build_signed_for_host(
        reader.clone(),
        "module.wasm",
        vec![module("module.wasm")],
        &SEED,
        &[HOST_API_V0_2],
    )
    .unwrap();
    assert_eq!(
        verify(&bytes, &publisher_key(&SEED)).unwrap_err().code,
        PackageErrorCode::Incompatible
    );
    assert_eq!(
        verify_for_host(&bytes, &publisher_key(&SEED), &SUPPORTED_HOST_APIS)
            .unwrap()
            .manifest(),
        &reader
    );
    for support in [
        vec![],
        vec![HOST_API, HOST_API],
        vec![extension_api::ApiVersion { major: 0, minor: 3 }],
    ] {
        assert_eq!(
            verify_for_host(&bytes, &publisher_key(&SEED), &support)
                .unwrap_err()
                .code,
            PackageErrorCode::Incompatible
        );
    }
    let mut corrupted = bytes;
    let position = corrupted.len() - 3;
    corrupted[position] = b'0';
    assert!(verify_for_host(&corrupted, &publisher_key(&SEED), &SUPPORTED_HOST_APIS).is_err());
}

#[test]
fn host_injected_mib_budgets_accept_real_rust_wasm_size_and_fail_closed() {
    let mut big_wasm = WASM.to_vec();
    big_wasm.resize(1_200_000, 0x00);
    let big_file = PackageFile {
        path: "module.wasm".to_owned(),
        mime: "application/wasm".to_owned(),
        bytes: big_wasm,
    };

    assert_eq!(
        build_signed(manifest(), "module.wasm", vec![big_file.clone()], &SEED)
            .unwrap_err()
            .code,
        PackageErrorCode::BudgetExceeded
    );

    let desktop = PackageBudgets::desktop_standard();
    let encoded = build_signed_with_budgets(
        manifest(),
        "module.wasm",
        vec![big_file],
        &SEED,
        &[HOST_API],
        &desktop,
    )
    .unwrap();
    assert_eq!(
        verify(&encoded, &publisher_key(&SEED)).unwrap_err().code,
        PackageErrorCode::BudgetExceeded
    );
    let verified =
        verify_with_budgets(&encoded, &publisher_key(&SEED), &[HOST_API], &desktop).unwrap();
    assert_eq!(verified.module_bytes().len(), 1_200_000);
    assert!(!verified.is_dev_unsigned());

    let mut invalid_ceiling = desktop;
    invalid_ceiling.max_package_bytes = HARD_MAX_PACKAGE_BYTES + 1;
    assert_eq!(
        verify_with_budgets(
            &encoded,
            &publisher_key(&SEED),
            &[HOST_API],
            &invalid_ceiling,
        )
        .unwrap_err()
        .code,
        PackageErrorCode::BudgetExceeded
    );

    let mut invalid_order = desktop;
    invalid_order.max_file_bytes = invalid_order.max_total_file_bytes + 1;
    assert_eq!(
        invalid_order.validate().unwrap_err().code,
        PackageErrorCode::BudgetExceeded
    );

    assert_eq!(dev_unsigned_supported(), cfg!(debug_assertions));
    assert!(!DevUnsignedOptions::default().allow_unsigned);
}

#[cfg(debug_assertions)]
#[test]
fn dev_unsigned_switch_requires_explicit_opt_in_and_keeps_structural_checks() {
    use extension_package::package::{
        build_dev_unsigned_with_budgets, verify_dev_unsigned_with_budgets,
    };

    assert!(dev_unsigned_supported());
    let budgets = PackageBudgets::mobile_standard();
    let unsigned_qpk = build_dev_unsigned_with_budgets(
        manifest(),
        "module.wasm",
        vec![module("module.wasm"), asset("assets/notice.txt")],
        &[HOST_API],
        &budgets,
    )
    .unwrap();

    // Default DevUnsignedOptions { allow_unsigned: false } must fail closed.
    assert_eq!(
        verify_dev_unsigned_with_budgets(
            &unsigned_qpk,
            &[HOST_API],
            &budgets,
            DevUnsignedOptions::default(),
        )
        .unwrap_err()
        .code,
        PackageErrorCode::UntrustedKey
    );

    // Production verify_with_budgets must always reject dev-unsigned packages.
    assert_eq!(
        verify_with_budgets(&unsigned_qpk, &publisher_key(&SEED), &[HOST_API], &budgets)
            .unwrap_err()
            .code,
        PackageErrorCode::Incompatible
    );

    // Explicit dev opt-in succeeds and surfaces is_dev_unsigned() == true.
    let verified = verify_dev_unsigned_with_budgets(
        &unsigned_qpk,
        &[HOST_API],
        &budgets,
        DevUnsignedOptions {
            allow_unsigned: true,
        },
    )
    .unwrap();
    assert!(verified.is_dev_unsigned());
    assert_eq!(verified.publisher_key(), "dev-unsigned");

    // Signed packages must not silently pass through the dev-unsigned gate.
    let signed_qpk = fixture();
    assert_eq!(
        verify_dev_unsigned_with_budgets(
            &signed_qpk,
            &[HOST_API],
            &budgets,
            DevUnsignedOptions {
                allow_unsigned: true,
            },
        )
        .unwrap_err()
        .code,
        PackageErrorCode::InvalidSignature
    );
}
