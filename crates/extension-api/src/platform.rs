//! Pure platform compatibility descriptors for F-EXT-07.
//!
//! A trusted host adapter supplies [`HostTarget`]; this module never probes the
//! operating system, reads configuration, or guesses a target with `cfg`. The
//! descriptors are static data only. Validating them or selecting one grants no
//! capability and does not mean that a runtime is installed, ready, or allowed.
//!
//! Target entries are alternatives (OR); the constraints inside one entry are
//! conjunctive. A portable Wasm artifact still has to list its operating systems
//! and channels explicitly.

use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    ApiError, ApiRange, ApiVersion, Capability, ErrorCode, decode_json, encode_json,
    validate_opaque_id,
};

/// Maximum ASCII byte length of a platform implementation or profile identifier.
pub const MAX_PLATFORM_ID_BYTES: usize = crate::MAX_ID_BYTES;
/// Maximum implementations in one resolver descriptor set.
pub const MAX_PLATFORM_IMPLEMENTATIONS: usize = 64;
/// Maximum target entries in one implementation.
pub const MAX_IMPLEMENTATION_TARGETS: usize = 64;
/// Maximum channels listed by one target entry.
pub const MAX_TARGET_CHANNELS: usize = 64;
/// Maximum required capabilities in one implementation.
pub const MAX_PLATFORM_CAPABILITIES: usize = 64;
/// Maximum encoded platform descriptor size in bytes.
pub const MAX_PLATFORM_DESCRIPTOR_BYTES: usize = 64 * 1024;
/// Version of this descriptor, independent from legacy Host manifests.
pub const PLATFORM_DESCRIPTOR_VERSION: u16 = 1;

/// Operating system named by an implementation target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingSystem {
    Macos,
    Windows,
    Linux,
    Ios,
    Android,
    Web,
}

/// Artifact architecture. Portable code still needs an explicit target OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    /// 64-bit Intel/AMD architecture.
    #[serde(rename = "x86_64")]
    X86_64,
    Aarch64,
    Wasm32,
}

/// A package may explicitly support every host CPU; the host's own architecture
/// remains concrete. Wasm bytecode architecture is not the host CPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetArchitecture {
    Any,
    #[serde(rename = "x86_64")]
    X86_64,
    Aarch64,
    Wasm32,
}
impl TargetArchitecture {
    pub fn matches(self, actual: Architecture) -> bool {
        self == Self::Any || self == Self::from(actual)
    }
}
impl From<Architecture> for TargetArchitecture {
    fn from(value: Architecture) -> Self {
        match value {
            Architecture::X86_64 => Self::X86_64,
            Architecture::Aarch64 => Self::Aarch64,
            Architecture::Wasm32 => Self::Wasm32,
        }
    }
}

/// Trusted application distribution channel. Plugins cannot report this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionChannel {
    DesktopDirect,
    MacAppStore,
    IosAppStore,
    AndroidPlay,
    Web,
}

/// Comparable operating system version tuple, ordered by major, minor, patch.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct OsVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl OsVersion {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

/// Trusted, already-probed host facts. The API does not derive or infer these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostTarget {
    pub os: OperatingSystem,
    pub arch: Architecture,
    pub os_version: OsVersion,
    pub channel: DistributionChannel,
}

/// One explicit platform alternative an implementation supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImplementationTarget {
    pub os: OperatingSystem,
    pub arch: TargetArchitecture,
    pub min_os_version: OsVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_os_version: Option<OsVersion>,
    /// Must be nonempty, explicit, and duplicate-free.
    pub channels: Vec<DistributionChannel>,
}

impl ImplementationTarget {
    /// Check the target's own range and channel declaration.
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.channels.is_empty() {
            return Err(invalid(
                "implementation target requires at least one channel",
            ));
        }
        if self.channels.len() > MAX_TARGET_CHANNELS {
            return Err(over_budget("too many target channels"));
        }
        let mut channels = BTreeSet::new();
        for channel in &self.channels {
            if !channels.insert(*channel) {
                return Err(invalid("duplicate target channel"));
            }
        }
        if self
            .max_os_version
            .is_some_and(|maximum| maximum < self.min_os_version)
        {
            return Err(invalid(
                "implementation target OS version range is inverted",
            ));
        }
        Ok(())
    }

    /// OS, architecture, and inclusive version range match. Channel is separate.
    pub fn matches_platform(&self, host: &HostTarget) -> bool {
        self.os == host.os
            && self.arch.matches(host.arch)
            && self.min_os_version <= host.os_version
            && self
                .max_os_version
                .is_none_or(|maximum| host.os_version <= maximum)
    }

    /// Platform and channel both match this target entry.
    pub fn matches(&self, host: &HostTarget) -> bool {
        self.matches_platform(host) && self.channels.contains(&host.channel)
    }
}

/// Bounded ASCII execution profile identifier supplied by trusted host policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExecutionProfile(String);

impl ExecutionProfile {
    /// Construct a validated execution profile identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, ApiError> {
        let value = value.into();
        validate_profile_id(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Revalidate a value created through the serde boundary.
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_profile_id(&self.0)
    }
}

impl fmt::Display for ExecutionProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for ExecutionProfile {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// One runnable artifact description inside a package release.
///
/// This is not a package envelope, an entry point, a digest, or an installation
/// record. The same implementation may target several operating systems.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformImplementation {
    pub descriptor_version: u16,
    pub implementation_id: String,
    /// Higher values are preferred only among otherwise eligible implementations.
    pub priority: u32,
    /// Must be nonempty; entries are alternative targets.
    pub targets: Vec<ImplementationTarget>,
    pub execution_profile: ExecutionProfile,
    /// Host API minor range exposed by this implementation.
    pub api_range: ApiRange,
    pub required_capabilities: Vec<Capability>,
}

impl PlatformImplementation {
    /// Validate the whole descriptor. This does not inspect the host or execute code.
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.descriptor_version != PLATFORM_DESCRIPTOR_VERSION {
            return Err(invalid("unsupported platform descriptor version"));
        }
        validate_platform_id(&self.implementation_id, "implementation_id")?;
        self.execution_profile.validate()?;
        if self.targets.is_empty() {
            return Err(invalid(
                "platform implementation requires at least one target",
            ));
        }
        if self.targets.len() > MAX_IMPLEMENTATION_TARGETS {
            return Err(over_budget("too many implementation targets"));
        }
        for target in &self.targets {
            target.validate()?;
        }
        if self.api_range.min_minor > self.api_range.max_minor {
            return Err(invalid("implementation api_range is inverted"));
        }
        if self.required_capabilities.len() > MAX_PLATFORM_CAPABILITIES {
            return Err(over_budget("too many required capabilities"));
        }
        let mut capabilities = BTreeSet::new();
        for capability in &self.required_capabilities {
            if !capabilities.insert(*capability) {
                return Err(invalid("duplicate required capability"));
            }
        }
        Ok(())
    }

    /// True when any target entry matches the host's OS, architecture, and version.
    /// Channels require a separate check on the matching target.
    pub fn matches_platform(&self, host: &HostTarget) -> bool {
        self.targets
            .iter()
            .any(|target| target.matches_platform(host))
    }

    /// True when a matching target also allows the host's distribution channel.
    pub fn allows_channel(&self, host: &HostTarget) -> bool {
        self.targets.iter().any(|target| target.matches(host))
    }

    pub fn supports_api(&self, version: ApiVersion) -> bool {
        self.api_range.supports(version)
    }

    /// Decode one bounded descriptor and validate it before returning it.
    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let implementation: Self = decode_json(
            json,
            MAX_PLATFORM_DESCRIPTOR_BYTES,
            ErrorCode::InvalidRequest,
        )?;
        implementation.validate()?;
        Ok(implementation)
    }

    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(
            self,
            MAX_PLATFORM_DESCRIPTOR_BYTES,
            ErrorCode::InvalidRequest,
        )
    }
}

/// Validate a nonempty descriptor set with at most 64 unique implementation IDs.
///
/// This is a pure structural check. It does not verify signatures, files, external
/// services, capabilities, or installation state.
pub fn validate_platform_implementations(
    implementations: &[PlatformImplementation],
) -> Result<(), ApiError> {
    if implementations.is_empty() {
        return Err(invalid("platform descriptor set is empty"));
    }
    if implementations.len() > MAX_PLATFORM_IMPLEMENTATIONS {
        return Err(over_budget("too many platform implementations"));
    }
    let mut ids = BTreeSet::new();
    for implementation in implementations {
        implementation.validate()?;
        if !ids.insert(implementation.implementation_id.as_str()) {
            return Err(invalid("duplicate implementation_id"));
        }
    }
    Ok(())
}

fn validate_platform_id(value: &str, field: &str) -> Result<(), ApiError> {
    validate_opaque_id(value, field)
}

fn validate_profile_id(value: &str) -> Result<(), ApiError> {
    validate_opaque_id(value, "execution profile")
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}

fn over_budget(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::BudgetExceeded, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_architecture_is_explicit_and_descriptor_version_is_checked() {
        let mut implementation = sample();
        implementation.targets[0].arch = TargetArchitecture::Any;
        let host = HostTarget {
            os: OperatingSystem::Macos,
            arch: Architecture::X86_64,
            os_version: OsVersion::new(14, 0, 0),
            channel: DistributionChannel::DesktopDirect,
        };
        assert!(implementation.matches_platform(&host));
        implementation.targets[0].os = OperatingSystem::Ios;
        assert!(!implementation.matches_platform(&host));
        implementation.descriptor_version = 2;
        assert!(implementation.validate().is_err());
        let mut value = serde_json::to_value(sample()).unwrap();
        value.as_object_mut().unwrap().remove("descriptor_version");
        assert!(PlatformImplementation::from_json(&value.to_string()).is_err());
    }

    fn sample() -> PlatformImplementation {
        PlatformImplementation {
            descriptor_version: PLATFORM_DESCRIPTOR_VERSION,
            implementation_id: "org.example.reader.native".into(),
            priority: 10,
            targets: vec![ImplementationTarget {
                os: OperatingSystem::Macos,
                arch: Architecture::Aarch64.into(),
                min_os_version: OsVersion::new(13, 0, 0),
                max_os_version: Some(OsVersion::new(14, 9, 9)),
                channels: vec![DistributionChannel::MacAppStore],
            }],
            execution_profile: ExecutionProfile::new("native").unwrap(),
            api_range: ApiRange {
                major: 0,
                min_minor: 1,
                max_minor: 3,
            },
            required_capabilities: vec![Capability::SelectionRead],
        }
    }

    #[test]
    fn operating_system_wire_identity_is_stable() {
        let cases = [
            (OperatingSystem::Macos, "macos"),
            (OperatingSystem::Windows, "windows"),
            (OperatingSystem::Linux, "linux"),
            (OperatingSystem::Ios, "ios"),
            (OperatingSystem::Android, "android"),
            (OperatingSystem::Web, "web"),
        ];
        for (value, wire) in cases {
            let encoded = serde_json::to_string(&value).unwrap();
            assert_eq!(encoded, format!("\"{wire}\""));
            let decoded: OperatingSystem = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, value);
        }
        assert_eq!(
            serde_json::to_string(&Architecture::X86_64).unwrap(),
            "\"x86_64\""
        );
        assert_eq!(
            serde_json::to_string(&Architecture::Aarch64).unwrap(),
            "\"aarch64\""
        );
        assert_eq!(
            serde_json::to_string(&Architecture::Wasm32).unwrap(),
            "\"wasm32\""
        );
        let channels = [
            (DistributionChannel::DesktopDirect, "desktop_direct"),
            (DistributionChannel::MacAppStore, "mac_app_store"),
            (DistributionChannel::IosAppStore, "ios_app_store"),
            (DistributionChannel::AndroidPlay, "android_play"),
            (DistributionChannel::Web, "web"),
        ];
        for (value, wire) in channels {
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                format!("\"{wire}\"")
            );
        }
    }

    #[test]
    fn unknown_fields_and_enum_values_are_rejected() {
        let base = serde_json::to_value(sample()).unwrap();

        let mut unknown_field = base.clone();
        unknown_field
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), true.into());
        assert!(
            PlatformImplementation::from_json(&unknown_field.to_string()).is_err(),
            "unknown top-level field must fail"
        );

        let mut unknown_os = base.clone();
        unknown_os["targets"][0]["os"] = "solaris".into();
        assert!(PlatformImplementation::from_json(&unknown_os.to_string()).is_err());

        let mut unknown_arch = base.clone();
        unknown_arch["targets"][0]["arch"] = "sparc".into();
        assert!(PlatformImplementation::from_json(&unknown_arch.to_string()).is_err());

        let mut unknown_channel = base.clone();
        unknown_channel["targets"][0]["channels"] = serde_json::json!(["floppy"]);
        assert!(PlatformImplementation::from_json(&unknown_channel.to_string()).is_err());

        let mut unknown_capability = base;
        unknown_capability["required_capabilities"] = serde_json::json!(["host.root"]);
        assert!(PlatformImplementation::from_json(&unknown_capability.to_string()).is_err());
    }

    #[test]
    fn versions_are_inclusive_and_matching_is_explicit() {
        assert!(OsVersion::new(13, 4, 9) < OsVersion::new(13, 5, 0));
        let host = HostTarget {
            os: OperatingSystem::Macos,
            arch: Architecture::Aarch64,
            os_version: OsVersion::new(13, 5, 0),
            channel: DistributionChannel::MacAppStore,
        };
        let target = sample().targets.remove(0);
        assert!(target.matches_platform(&host));
        assert!(target.matches(&host));

        let mut too_new = target.clone();
        too_new.max_os_version = Some(OsVersion::new(13, 4, 9));
        assert!(!too_new.matches_platform(&host));

        let mut too_old = target.clone();
        too_old.min_os_version = OsVersion::new(13, 5, 1);
        assert!(!too_old.matches_platform(&host));

        let mut other_os = target.clone();
        other_os.os = OperatingSystem::Windows;
        assert!(!other_os.matches_platform(&host));

        let mut other_arch = target;
        other_arch.arch = Architecture::X86_64.into();
        assert!(!other_arch.matches_platform(&host));
    }

    #[test]
    fn malformed_or_empty_descriptors_are_rejected() {
        let valid = sample();
        assert!(valid.validate().is_ok());
        assert!(validate_platform_implementations(std::slice::from_ref(&valid)).is_ok());
        assert_eq!(
            validate_platform_implementations(&[]).unwrap_err().code,
            ErrorCode::InvalidRequest
        );

        let mut empty_targets = valid.clone();
        empty_targets.targets.clear();
        assert_eq!(
            empty_targets.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );

        let mut empty_channels = valid.clone();
        empty_channels.targets[0].channels.clear();
        assert_eq!(
            empty_channels.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );

        let mut duplicate_channel = valid.clone();
        duplicate_channel.targets[0].channels = vec![
            DistributionChannel::MacAppStore,
            DistributionChannel::MacAppStore,
        ];
        assert!(duplicate_channel.validate().is_err());

        let mut inverted_range = valid.clone();
        inverted_range.api_range.min_minor = 4;
        assert!(inverted_range.validate().is_err());

        let mut inverted_os_range = valid.clone();
        inverted_os_range.targets[0].max_os_version = Some(OsVersion::new(12, 9, 9));
        assert!(inverted_os_range.validate().is_err());

        let mut duplicate_capability = valid.clone();
        duplicate_capability.required_capabilities =
            vec![Capability::SelectionRead, Capability::SelectionRead];
        assert!(duplicate_capability.validate().is_err());

        let mut non_ascii_id = valid.clone();
        non_ascii_id.implementation_id = "org.example.阅读".into();
        assert!(non_ascii_id.validate().is_err());

        let mut duplicate_ids = valid.clone();
        duplicate_ids.implementation_id = valid.implementation_id.clone();
        assert_eq!(
            validate_platform_implementations(&[valid.clone(), duplicate_ids])
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );

        let too_many: Vec<_> = (0..=MAX_PLATFORM_IMPLEMENTATIONS)
            .map(|index| {
                let mut implementation = valid.clone();
                implementation.implementation_id = format!("org.example.impl{index}");
                implementation
            })
            .collect();
        assert_eq!(
            validate_platform_implementations(&too_many)
                .unwrap_err()
                .code,
            ErrorCode::BudgetExceeded
        );

        assert!(ExecutionProfile::new("native").is_ok());
        assert!(ExecutionProfile::new("").is_err());
        assert!(ExecutionProfile::new("native profile").is_err());
    }
}
