use capability_composition::{
    CompositionError, CompositionLayer, CompositionProfile, Entry, MountError, RuntimeScope,
    ServiceId, ServiceProvision, ServiceRequirement, ServiceVersion, VersionRequirement,
};

fn service(value: &str) -> ServiceId {
    ServiceId::parse(value).unwrap()
}

fn provision(value: &str, major: u16, minor: u16) -> ServiceProvision {
    ServiceProvision::new(service(value), ServiceVersion::new(major, minor))
}

fn requirement(value: &str, major: u16, min_minor: u16) -> ServiceRequirement {
    ServiceRequirement::new(service(value), VersionRequirement::new(major, min_minor))
}

fn entry(id: &str, package: &str) -> Entry {
    Entry::new(id, package).unwrap()
}

fn profile(layers: Vec<CompositionLayer>) -> CompositionProfile {
    let mut profile = CompositionProfile::new("desktop").unwrap();
    for layer in layers {
        profile.push_layer(layer).unwrap();
    }
    profile
}

#[test]
fn providers_activate_before_consumers_and_override_layers_keep_position() {
    let base = CompositionLayer::new(
        "base",
        vec![
            entry("app.host", "org.qusor.host")
                .required(true)
                .providing(provision("notes.read", 0, 4)),
            entry("app.workspace", "org.qusor.notes").requiring(requirement("notes.read", 0, 4)),
        ],
    )
    .unwrap();
    let features = CompositionLayer::new(
        "features",
        vec![
            entry("app.workspace", "org.qusor.notes")
                .requiring(requirement("notes.read", 0, 4))
                .requiring(requirement("qusor.vault", 0, 1)),
            entry("app.vault", "org.qusor.core").providing(provision("qusor.vault", 0, 1)),
        ],
    )
    .unwrap();

    let plan = profile(vec![base, features]).resolve().unwrap();
    let ids = plan
        .entries()
        .iter()
        .map(|planned| planned.entry().id().as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["app.host", "app.vault", "app.workspace"]);
    assert_eq!(
        plan.entry(&capability_composition::EntryId::parse("app.workspace").unwrap())
            .unwrap()
            .layer()
            .as_str(),
        "features"
    );
    let dump = plan.dump();
    assert!(dump.contains("profile desktop"));
    assert!(dump.contains("app.workspace [features]"));
    assert!(dump.contains("requires qusor.vault >=0.1"));
}

#[test]
fn missing_provider_is_rejected() {
    let profile = profile(vec![
        CompositionLayer::new(
            "base",
            vec![
                entry("app.workspace", "org.qusor.notes").requiring(requirement(
                    "notes.read",
                    0,
                    4,
                )),
            ],
        )
        .unwrap(),
    ]);
    let error = profile.resolve().unwrap_err();
    assert!(matches!(
        error,
        CompositionError::MissingProvider { ref entry, ref service }
            if entry.as_str() == "app.workspace" && service.as_str() == "notes.read"
    ));
}

#[test]
fn incompatible_provider_is_rejected() {
    let profile = profile(vec![
        CompositionLayer::new(
            "base",
            vec![
                entry("app.host", "org.qusor.host").providing(provision("notes.read", 0, 3)),
                entry("app.workspace", "org.qusor.notes").requiring(requirement(
                    "notes.read",
                    0,
                    4,
                )),
            ],
        )
        .unwrap(),
    ]);
    let error = profile.resolve().unwrap_err();
    assert!(matches!(
        error,
        CompositionError::IncompatibleProvider { ref entry, .. }
            if entry.as_str() == "app.workspace"
    ));
}

#[test]
fn duplicate_provider_is_rejected() {
    let profile = profile(vec![
        CompositionLayer::new(
            "base",
            vec![
                entry("app.host-a", "org.qusor.host-a").providing(provision("notes.read", 0, 4)),
                entry("app.host-b", "org.qusor.host-b").providing(provision("notes.read", 0, 4)),
            ],
        )
        .unwrap(),
    ]);
    let error = profile.resolve().unwrap_err();
    assert!(matches!(
        error,
        CompositionError::DuplicateProvider { ref service, .. }
            if service.as_str() == "notes.read"
    ));
}

#[test]
fn required_entry_cannot_be_disabled_but_optional_entry_can() {
    let layers = vec![
        CompositionLayer::new(
            "base",
            vec![
                entry("app.required", "org.qusor.required").required(true),
                entry("app.optional", "org.qusor.optional"),
            ],
        )
        .unwrap(),
        CompositionLayer::new(
            "user",
            vec![
                entry("app.required", "org.qusor.required")
                    .required(true)
                    .enabled(false),
                entry("app.optional", "org.qusor.optional").enabled(false),
            ],
        )
        .unwrap(),
    ];
    let error = profile(layers).resolve().unwrap_err();
    assert!(matches!(
        error,
        CompositionError::RequiredEntryDisabled { ref entry }
            if entry.as_str() == "app.required"
    ));
}

#[test]
fn dependency_cycles_are_rejected() {
    let profile = profile(vec![
        CompositionLayer::new(
            "base",
            vec![
                entry("app.a", "org.qusor.a")
                    .providing(provision("qusor.a", 0, 1))
                    .requiring(requirement("qusor.b", 0, 1)),
                entry("app.b", "org.qusor.b")
                    .providing(provision("qusor.b", 0, 1))
                    .requiring(requirement("qusor.a", 0, 1)),
            ],
        )
        .unwrap(),
    ]);
    let error = profile.resolve().unwrap_err();
    assert!(matches!(error, CompositionError::DependencyCycle { .. }));
}

#[test]
fn repeated_service_in_one_entry_is_rejected() {
    let profile = profile(vec![
        CompositionLayer::new(
            "base",
            vec![
                entry("app.host", "org.qusor.host")
                    .providing(provision("notes.read", 0, 4))
                    .providing(provision("notes.read", 0, 4)),
            ],
        )
        .unwrap(),
    ]);
    let error = profile.resolve().unwrap_err();
    assert!(matches!(
        error,
        CompositionError::DuplicateServiceInEntry { ref service, .. }
            if service.as_str() == "notes.read"
    ));
}

#[test]
fn malformed_identifiers_are_rejected() {
    assert!(Entry::new("Bad ID", "org.qusor.host").is_err());
    assert!(ServiceId::parse("..").is_err());
    assert!(CompositionProfile::new("QUSOR").is_err());
}

#[test]
fn runtime_scope_mounts_in_plan_order_and_releases_in_reverse() {
    let plan = profile(vec![
        CompositionLayer::new(
            "base",
            vec![
                entry("app.core", "org.qusor.core"),
                entry("app.host", "org.qusor.host"),
                entry("app.workspace", "org.qusor.notes"),
            ],
        )
        .unwrap(),
    ])
    .resolve()
    .unwrap();
    let mut scope = RuntimeScope::new(plan);
    let core = capability_composition::EntryId::parse("app.core").unwrap();
    let host = capability_composition::EntryId::parse("app.host").unwrap();
    let workspace = capability_composition::EntryId::parse("app.workspace").unwrap();

    scope.mount(&core, "core").unwrap();
    scope.mount(&host, "host").unwrap();
    let error = scope.mount(&core, "again");
    assert!(matches!(error, Err(MountError::DuplicateMount { .. })));
    let error = scope.mount(
        &capability_composition::EntryId::parse("app.workspace").unwrap(),
        "workspace",
    );
    assert!(error.is_ok());
    assert_eq!(scope.get(&core), Some(&"core"));
    assert_eq!(scope.get(&host), Some(&"host"));
    let released = scope.unmount_all();
    assert_eq!(
        released
            .iter()
            .map(|mounted| mounted.value())
            .copied()
            .collect::<Vec<_>>(),
        ["workspace", "host", "core"]
    );
    assert!(scope.mounted().is_empty());
    assert!(scope.get(&workspace).is_none());
}

#[test]
fn runtime_scope_rejects_unknown_entries_and_reverse_order() {
    let plan = profile(vec![
        CompositionLayer::new(
            "base",
            vec![
                entry("app.core", "org.qusor.core"),
                entry("app.host", "org.qusor.host"),
            ],
        )
        .unwrap(),
    ])
    .resolve()
    .unwrap();
    let mut scope = RuntimeScope::new(plan);
    let core = capability_composition::EntryId::parse("app.core").unwrap();
    let host = capability_composition::EntryId::parse("app.host").unwrap();
    assert!(matches!(
        scope.mount(
            &capability_composition::EntryId::parse("app.missing").unwrap(),
            "missing"
        ),
        Err(MountError::UnknownEntry { .. })
    ));
    scope.mount(&host, "host").unwrap();
    assert!(matches!(
        scope.mount(&core, "core"),
        Err(MountError::OutOfOrder { .. })
    ));
}
