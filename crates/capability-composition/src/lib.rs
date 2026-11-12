//! Capability seams and declarative composition for QUSOR.
//!
//! This crate is deliberately independent of Cargo, NSH, SQL, Host policy and
//! plugin runtimes. It answers one question: given ordered profile layers, which
//! enabled entries provide which versioned services, and in what order may they
//! activate without missing providers, duplicate providers or dependency cycles?
//!
//! A live provider is not authorized by resolving this graph. Host grants,
//! package signatures, process/Wasm isolation and Core transactions remain
//! separate concerns.
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Write},
    str::FromStr,
};

pub const MAX_ENTRIES: usize = 128;
pub const MAX_SERVICES_PER_ENTRY: usize = 32;
pub const MAX_IDENTIFIER_BYTES: usize = 128;

fn validate_identifier(value: &str, max: usize) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("must not be empty");
    }
    if value.len() > max {
        return Err("is too long");
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
    }) {
        return Err("must use lowercase ASCII letters, digits, '.', '-' or '_'");
    }
    let starts_with_separator = value
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '.' | '-' | '_'));
    let ends_with_separator = value
        .chars()
        .next_back()
        .is_some_and(|character| matches!(character, '.' | '-' | '_'));
    if starts_with_separator || ends_with_separator {
        return Err("must not start or end with a separator");
    }
    if value.contains("..") || value.contains("--") || value.contains("__") {
        return Err("must not contain adjacent separators");
    }
    Ok(())
}

macro_rules! identifier {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl AsRef<str>) -> Result<Self, CompositionError> {
                let value = value.as_ref();
                validate_identifier(value, MAX_IDENTIFIER_BYTES).map_err(|reason| {
                    CompositionError::InvalidIdentifier {
                        kind: $kind,
                        value: value.to_owned(),
                        reason,
                    }
                })?;
                Ok(Self(value.to_owned()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = CompositionError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }
    };
}

identifier!(EntryId, "entry id");
identifier!(LayerId, "layer id");
identifier!(PackageId, "package id");
identifier!(ServiceId, "service id");
identifier!(ProfileId, "profile id");

/// A provider version. Compatibility is major-exact and minor-backward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServiceVersion {
    pub major: u16,
    pub minor: u16,
}

impl ServiceVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for ServiceVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

/// A consumer requirement: same major and at least `min_minor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionRequirement {
    pub major: u16,
    pub min_minor: u16,
}

impl VersionRequirement {
    pub const fn new(major: u16, min_minor: u16) -> Self {
        Self { major, min_minor }
    }

    pub const fn matches(self, provided: ServiceVersion) -> bool {
        self.major == provided.major && self.min_minor <= provided.minor
    }
}

impl fmt::Display for VersionRequirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, ">={}.{}", self.major, self.min_minor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceProvision {
    service: ServiceId,
    version: ServiceVersion,
}

impl ServiceProvision {
    pub fn new(service: ServiceId, version: ServiceVersion) -> Self {
        Self { service, version }
    }

    pub fn service(&self) -> &ServiceId {
        &self.service
    }

    pub fn version(&self) -> ServiceVersion {
        self.version
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceRequirement {
    service: ServiceId,
    version: VersionRequirement,
}

impl ServiceRequirement {
    pub fn new(service: ServiceId, version: VersionRequirement) -> Self {
        Self { service, version }
    }

    pub fn service(&self) -> &ServiceId {
        &self.service
    }

    pub fn version(&self) -> VersionRequirement {
        self.version
    }
}

/// One profile row. A later layer may replace the complete row by entry id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    id: EntryId,
    package: PackageId,
    enabled: bool,
    required: bool,
    provides: Vec<ServiceProvision>,
    requires: Vec<ServiceRequirement>,
}

impl Entry {
    pub fn new(id: impl AsRef<str>, package: impl AsRef<str>) -> Result<Self, CompositionError> {
        Ok(Self {
            id: EntryId::parse(id)?,
            package: PackageId::parse(package)?,
            enabled: true,
            required: false,
            provides: Vec::new(),
            requires: Vec::new(),
        })
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn providing(mut self, provision: ServiceProvision) -> Self {
        self.provides.push(provision);
        self
    }

    pub fn requiring(mut self, requirement: ServiceRequirement) -> Self {
        self.requires.push(requirement);
        self
    }

    pub fn id(&self) -> &EntryId {
        &self.id
    }

    pub fn package(&self) -> &PackageId {
        &self.package
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_required(&self) -> bool {
        self.required
    }

    pub fn provides(&self) -> &[ServiceProvision] {
        &self.provides
    }

    pub fn requires(&self) -> &[ServiceRequirement] {
        &self.requires
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionLayer {
    id: LayerId,
    entries: Vec<Entry>,
}

impl CompositionLayer {
    pub fn new(id: impl AsRef<str>, entries: Vec<Entry>) -> Result<Self, CompositionError> {
        if entries.len() > MAX_ENTRIES {
            return Err(CompositionError::TooManyEntries {
                count: entries.len(),
                limit: MAX_ENTRIES,
            });
        }
        Ok(Self {
            id: LayerId::parse(id)?,
            entries,
        })
    }

    pub fn id(&self) -> &LayerId {
        &self.id
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

/// Ordered profile layers. Later layers replace a complete row by id and retain
/// the row's original position, matching deterministic application semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionProfile {
    id: ProfileId,
    layers: Vec<CompositionLayer>,
}

impl CompositionProfile {
    pub fn new(id: impl AsRef<str>) -> Result<Self, CompositionError> {
        Ok(Self {
            id: ProfileId::parse(id)?,
            layers: Vec::new(),
        })
    }

    pub fn push_layer(&mut self, layer: CompositionLayer) -> Result<(), CompositionError> {
        if self.layers.iter().any(|existing| existing.id == layer.id) {
            return Err(CompositionError::DuplicateLayer { layer: layer.id });
        }
        self.layers.push(layer);
        Ok(())
    }

    pub fn with_layer(mut self, layer: CompositionLayer) -> Result<Self, CompositionError> {
        self.push_layer(layer)?;
        Ok(self)
    }

    pub fn id(&self) -> &ProfileId {
        &self.id
    }

    pub fn layers(&self) -> &[CompositionLayer] {
        &self.layers
    }

    /// Resolve layers, validate service seams and return a stable activation plan.
    pub fn resolve(&self) -> Result<CompositionPlan, CompositionError> {
        let mut rows = Vec::<ResolvedEntry>::new();
        let mut positions = BTreeMap::<EntryId, usize>::new();
        let mut seen_layers = BTreeSet::new();

        for layer in &self.layers {
            if !seen_layers.insert(layer.id.clone()) {
                return Err(CompositionError::DuplicateLayer {
                    layer: layer.id.clone(),
                });
            }
            let mut seen_entries = BTreeSet::new();
            for entry in &layer.entries {
                if !seen_entries.insert(entry.id.clone()) {
                    return Err(CompositionError::DuplicateEntryInLayer {
                        layer: layer.id.clone(),
                        entry: entry.id.clone(),
                    });
                }
                let row = ResolvedEntry {
                    entry: entry.clone(),
                    layer: layer.id.clone(),
                };
                if let Some(index) = positions.get(&entry.id).copied() {
                    rows[index] = row;
                } else {
                    positions.insert(entry.id.clone(), rows.len());
                    rows.push(row);
                }
            }
        }

        if rows.len() > MAX_ENTRIES {
            return Err(CompositionError::TooManyEntries {
                count: rows.len(),
                limit: MAX_ENTRIES,
            });
        }

        for row in &rows {
            validate_entry_services(&row.entry)?;
            if row.entry.required && !row.entry.enabled {
                return Err(CompositionError::RequiredEntryDisabled {
                    entry: row.entry.id.clone(),
                });
            }
        }

        let mut providers = BTreeMap::<ServiceId, ResolvedProvider>::new();
        for (index, row) in rows.iter().enumerate() {
            if !row.entry.enabled {
                continue;
            }
            for provision in &row.entry.provides {
                if let Some(first) = providers.get(provision.service()) {
                    return Err(CompositionError::DuplicateProvider {
                        service: provision.service().clone(),
                        first: first.entry.clone(),
                        second: row.entry.id.clone(),
                    });
                }
                providers.insert(
                    provision.service().clone(),
                    ResolvedProvider {
                        entry: row.entry.id.clone(),
                        version: provision.version(),
                        index,
                    },
                );
            }
        }

        let mut dependencies = vec![BTreeSet::<usize>::new(); rows.len()];
        for (index, row) in rows.iter().enumerate() {
            if !row.entry.enabled {
                continue;
            }
            for requirement in &row.entry.requires {
                let Some(provider) = providers.get(requirement.service()) else {
                    return Err(CompositionError::MissingProvider {
                        entry: row.entry.id.clone(),
                        service: requirement.service().clone(),
                    });
                };
                if !requirement.version().matches(provider.version) {
                    return Err(CompositionError::IncompatibleProvider {
                        entry: row.entry.id.clone(),
                        service: requirement.service().clone(),
                        required: requirement.version(),
                        provided: provider.version,
                    });
                }
                dependencies[index].insert(provider.index);
            }
        }

        let ordered = activation_order(&rows, &dependencies)?;
        let entries = ordered
            .into_iter()
            .map(|index| PlannedEntry {
                entry: rows[index].entry.clone(),
                layer: rows[index].layer.clone(),
            })
            .collect();
        let providers = providers
            .into_iter()
            .map(|(service, provider)| {
                (
                    service,
                    ProviderBinding {
                        entry: provider.entry,
                        version: provider.version,
                    },
                )
            })
            .collect();
        Ok(CompositionPlan {
            profile: self.id.clone(),
            entries,
            providers,
        })
    }
}

fn validate_entry_services(entry: &Entry) -> Result<(), CompositionError> {
    if entry.provides.len() > MAX_SERVICES_PER_ENTRY {
        return Err(CompositionError::TooManyServices {
            entry: entry.id.clone(),
            kind: "provided",
            count: entry.provides.len(),
            limit: MAX_SERVICES_PER_ENTRY,
        });
    }
    if entry.requires.len() > MAX_SERVICES_PER_ENTRY {
        return Err(CompositionError::TooManyServices {
            entry: entry.id.clone(),
            kind: "required",
            count: entry.requires.len(),
            limit: MAX_SERVICES_PER_ENTRY,
        });
    }
    let mut provided = BTreeSet::new();
    for provision in &entry.provides {
        if !provided.insert(provision.service()) {
            return Err(CompositionError::DuplicateServiceInEntry {
                entry: entry.id.clone(),
                service: provision.service().clone(),
                kind: "provided",
            });
        }
    }
    let mut required = BTreeSet::new();
    for requirement in &entry.requires {
        if !required.insert(requirement.service()) {
            return Err(CompositionError::DuplicateServiceInEntry {
                entry: entry.id.clone(),
                service: requirement.service().clone(),
                kind: "required",
            });
        }
    }
    Ok(())
}

fn activation_order(
    rows: &[ResolvedEntry],
    dependencies: &[BTreeSet<usize>],
) -> Result<Vec<usize>, CompositionError> {
    let enabled = rows.iter().filter(|row| row.entry.enabled).count();
    let mut indegree = vec![0usize; rows.len()];
    let mut dependents = vec![Vec::<usize>::new(); rows.len()];
    for (index, deps) in dependencies.iter().enumerate() {
        indegree[index] = deps.len();
        for dependency in deps {
            dependents[*dependency].push(index);
        }
    }
    let mut ready = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        if row.entry.enabled && indegree[index] == 0 {
            ready.insert(index);
        }
    }
    let mut ordered = Vec::with_capacity(enabled);
    while let Some(index) = ready.pop_first() {
        ordered.push(index);
        for dependent in &dependents[index] {
            indegree[*dependent] -= 1;
            if indegree[*dependent] == 0 {
                ready.insert(*dependent);
            }
        }
    }
    if ordered.len() != enabled {
        let cycle = rows
            .iter()
            .enumerate()
            .filter(|(index, row)| row.entry.enabled && indegree[*index] > 0)
            .map(|(_, row)| row.entry.id.clone())
            .collect();
        return Err(CompositionError::DependencyCycle { entries: cycle });
    }
    Ok(ordered)
}

#[derive(Clone, Debug)]
struct ResolvedEntry {
    entry: Entry,
    layer: LayerId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedEntry {
    entry: Entry,
    layer: LayerId,
}

impl PlannedEntry {
    pub fn entry(&self) -> &Entry {
        &self.entry
    }

    pub fn layer(&self) -> &LayerId {
        &self.layer
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderBinding {
    entry: EntryId,
    version: ServiceVersion,
}

impl ProviderBinding {
    pub fn entry(&self) -> &EntryId {
        &self.entry
    }

    pub fn version(&self) -> ServiceVersion {
        self.version
    }
}

#[derive(Clone, Debug)]
struct ResolvedProvider {
    entry: EntryId,
    version: ServiceVersion,
    index: usize,
}

/// Validated plan. Disabled rows are omitted; required rows cannot be disabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionPlan {
    profile: ProfileId,
    entries: Vec<PlannedEntry>,
    providers: BTreeMap<ServiceId, ProviderBinding>,
}

impl CompositionPlan {
    pub fn profile(&self) -> &ProfileId {
        &self.profile
    }

    pub fn entries(&self) -> &[PlannedEntry] {
        &self.entries
    }

    pub fn entry(&self, id: &EntryId) -> Option<&PlannedEntry> {
        self.entries.iter().find(|planned| planned.entry.id == *id)
    }

    pub fn provider(&self, service: &ServiceId) -> Option<&ProviderBinding> {
        self.providers.get(service)
    }

    /// Deterministic human-readable diagnostics for a settings/help surface.
    pub fn dump(&self) -> String {
        let mut output = format!(
            "profile {} ({} active entries)\n",
            self.profile,
            self.entries.len()
        );
        for (index, planned) in self.entries.iter().enumerate() {
            let entry = &planned.entry;
            let required = if entry.required { " required" } else { "" };
            let _ = writeln!(
                output,
                "{index:02} {} [{}] package={}{required}",
                entry.id, planned.layer, entry.package
            );
            for provision in &entry.provides {
                let _ = writeln!(
                    output,
                    "   provides {} {}",
                    provision.service(),
                    provision.version()
                );
            }
            for requirement in &entry.requires {
                let _ = writeln!(
                    output,
                    "   requires {} {}",
                    requirement.service(),
                    requirement.version()
                );
            }
        }
        output
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositionError {
    InvalidIdentifier {
        kind: &'static str,
        value: String,
        reason: &'static str,
    },
    DuplicateLayer {
        layer: LayerId,
    },
    DuplicateEntryInLayer {
        layer: LayerId,
        entry: EntryId,
    },
    TooManyEntries {
        count: usize,
        limit: usize,
    },
    TooManyServices {
        entry: EntryId,
        kind: &'static str,
        count: usize,
        limit: usize,
    },
    DuplicateServiceInEntry {
        entry: EntryId,
        service: ServiceId,
        kind: &'static str,
    },
    RequiredEntryDisabled {
        entry: EntryId,
    },
    DuplicateProvider {
        service: ServiceId,
        first: EntryId,
        second: EntryId,
    },
    MissingProvider {
        entry: EntryId,
        service: ServiceId,
    },
    IncompatibleProvider {
        entry: EntryId,
        service: ServiceId,
        required: VersionRequirement,
        provided: ServiceVersion,
    },
    DependencyCycle {
        entries: Vec<EntryId>,
    },
}

impl fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier {
                kind,
                value,
                reason,
            } => write!(formatter, "invalid {kind} {value:?}: {reason}"),
            Self::DuplicateLayer { layer } => {
                write!(formatter, "duplicate composition layer: {layer}")
            }
            Self::DuplicateEntryInLayer { layer, entry } => {
                write!(formatter, "duplicate entry {entry} in layer {layer}")
            }
            Self::TooManyEntries { count, limit } => {
                write!(
                    formatter,
                    "composition has {count} entries; limit is {limit}"
                )
            }
            Self::TooManyServices {
                entry,
                kind,
                count,
                limit,
            } => write!(
                formatter,
                "entry {entry} has {count} {kind} services; limit is {limit}"
            ),
            Self::DuplicateServiceInEntry {
                entry,
                service,
                kind,
            } => write!(
                formatter,
                "entry {entry} declares service {service} more than once in its {kind} set"
            ),
            Self::RequiredEntryDisabled { entry } => {
                write!(formatter, "required entry is disabled: {entry}")
            }
            Self::DuplicateProvider {
                service,
                first,
                second,
            } => write!(
                formatter,
                "service {service} has multiple providers: {first} and {second}"
            ),
            Self::MissingProvider { entry, service } => {
                write!(
                    formatter,
                    "entry {entry} requires missing service {service}"
                )
            }
            Self::IncompatibleProvider {
                entry,
                service,
                required,
                provided,
            } => write!(
                formatter,
                "entry {entry} requires {service} {required}, but provider offers {provided}"
            ),
            Self::DependencyCycle { entries } => {
                let entries = entries
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "composition dependency cycle: {entries}")
            }
        }
    }
}

impl Error for CompositionError {}
