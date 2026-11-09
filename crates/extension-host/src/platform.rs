//! Pure platform implementation resolution for F-EXT-07.
//!
//! Resolution consumes trusted host facts and already-verified descriptors. It
//! does not probe the OS, inspect files, execute plugin code, grant capabilities,
//! or claim that an implementation is installed, ready, or authorized.
//!
//! A result is only a selection decision. Runtime readiness, permissions, external
//! dependencies, signatures, and installation state remain separate host checks.

use std::fmt;

use extension_api::{
    ApiVersion, Capability,
    platform::{
        ExecutionProfile, HostTarget, PlatformImplementation, validate_platform_implementations,
    },
};

/// Structured reason that an implementation could not be selected.
///
/// This is not a plugin health status and carries no grants or readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformFailure {
    /// No implementation targets the host OS, architecture, and version.
    UnsupportedTarget,
    /// A target matches, but no listed runtime profile or API version is available.
    UnsupportedRuntime,
    /// A target matches, but its explicit channels exclude the host channel.
    ChannelRestricted,
    /// The implementation requires a capability the trusted host did not list.
    MissingCapability,
    /// The user-pinned implementation does not exist or is not eligible.
    /// Resolution never substitutes a different implementation.
    PinnedUnavailable,
    /// The descriptor set is empty, malformed, duplicated, or over budget.
    InvalidDescriptor,
}

impl fmt::Display for PlatformFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnsupportedTarget => "unsupported platform target",
            Self::UnsupportedRuntime => "unsupported runtime profile or API",
            Self::ChannelRestricted => "distribution channel restricted",
            Self::MissingCapability => "required capability missing",
            Self::PinnedUnavailable => "pinned implementation unavailable",
            Self::InvalidDescriptor => "invalid platform descriptor",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for PlatformFailure {}

/// Trusted resolver inputs. All references are borrowed for the resolution call.
///
/// `execution_profiles`, `api_versions`, and `capabilities` come from the trusted
/// host. `pinned_implementation_id` is an explicit user choice, not a fallback hint.
#[derive(Debug, Clone, Copy)]
pub struct PlatformResolver<'a> {
    pub host: &'a HostTarget,
    pub execution_profiles: &'a [ExecutionProfile],
    pub api_versions: &'a [ApiVersion],
    pub capabilities: &'a [Capability],
    pub implementations: &'a [PlatformImplementation],
    pub pinned_implementation_id: Option<&'a str>,
}

impl<'a> PlatformResolver<'a> {
    /// Select one implementation, or a stable structured failure.
    ///
    /// Candidate order is priority descending, then implementation ID ascending
    /// lexicographically. The first fully eligible candidate wins, so a lower
    /// priority candidate can be chosen when a higher one is blocked. If none is
    /// eligible, the reason from the first blocked target-matching candidate is
    /// returned; checks are runtime, channel, then capability.
    ///
    /// A pin disables this search entirely. A missing or ineligible pin returns
    /// [`PlatformFailure::PinnedUnavailable`] even when another candidate is valid.
    pub fn resolve(&self) -> Result<&'a PlatformImplementation, PlatformFailure> {
        validate_platform_implementations(self.implementations)
            .map_err(|_| PlatformFailure::InvalidDescriptor)?;

        if let Some(pinned) = self.pinned_implementation_id {
            let candidate = self
                .implementations
                .iter()
                .find(|implementation| implementation.implementation_id == pinned)
                .ok_or(PlatformFailure::PinnedUnavailable)?;
            return self
                .check(candidate)
                .map(|()| candidate)
                .map_err(|_| PlatformFailure::PinnedUnavailable);
        }

        let mut matching: Vec<_> = self
            .implementations
            .iter()
            .filter(|implementation| implementation.matches_platform(self.host))
            .collect();
        if matching.is_empty() {
            return Err(PlatformFailure::UnsupportedTarget);
        }
        matching.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.implementation_id.cmp(&right.implementation_id))
        });

        let mut first_failure = None;
        for candidate in matching {
            match self.check(candidate) {
                Ok(()) => return Ok(candidate),
                Err(failure) => {
                    if first_failure.is_none() {
                        first_failure = Some(failure);
                    }
                }
            }
        }
        Err(first_failure.unwrap_or(PlatformFailure::UnsupportedTarget))
    }

    fn check(&self, candidate: &PlatformImplementation) -> Result<(), PlatformFailure> {
        if !candidate.matches_platform(self.host) {
            return Err(PlatformFailure::UnsupportedTarget);
        }
        if !self
            .execution_profiles
            .contains(&candidate.execution_profile)
            || !self
                .api_versions
                .iter()
                .any(|version| candidate.supports_api(*version))
        {
            return Err(PlatformFailure::UnsupportedRuntime);
        }
        if !candidate.allows_channel(self.host) {
            return Err(PlatformFailure::ChannelRestricted);
        }
        if candidate
            .required_capabilities
            .iter()
            .any(|capability| !self.capabilities.contains(capability))
        {
            return Err(PlatformFailure::MissingCapability);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use extension_api::{
        ApiRange, HOST_API_V0_3,
        platform::{
            Architecture, DistributionChannel, ExecutionProfile, HostTarget, ImplementationTarget,
            OperatingSystem, OsVersion, PlatformImplementation,
        },
    };

    use super::*;

    fn version(major: u16, minor: u16, patch: u16) -> OsVersion {
        OsVersion::new(major, minor, patch)
    }

    fn target(
        os: OperatingSystem,
        arch: Architecture,
        channels: &[DistributionChannel],
    ) -> ImplementationTarget {
        ImplementationTarget {
            os,
            arch: arch.into(),
            min_os_version: version(1, 0, 0),
            max_os_version: None,
            channels: channels.to_vec(),
        }
    }

    fn implementation(
        id: &str,
        priority: u32,
        targets: Vec<ImplementationTarget>,
    ) -> PlatformImplementation {
        PlatformImplementation {
            descriptor_version: extension_api::platform::PLATFORM_DESCRIPTOR_VERSION,
            implementation_id: id.into(),
            priority,
            targets,
            execution_profile: ExecutionProfile::new("native").unwrap(),
            api_range: ApiRange {
                major: 0,
                min_minor: 1,
                max_minor: 3,
            },
            required_capabilities: vec![Capability::SelectionRead],
        }
    }

    fn host(os: OperatingSystem, arch: Architecture, channel: DistributionChannel) -> HostTarget {
        HostTarget {
            os,
            arch,
            os_version: version(14, 0, 0),
            channel,
        }
    }

    fn resolve<'a>(
        host: &'a HostTarget,
        implementations: &'a [PlatformImplementation],
        profiles: &'a [ExecutionProfile],
        api_versions: &'a [ApiVersion],
        capabilities: &'a [Capability],
        pinned: Option<&'a str>,
    ) -> Result<&'a PlatformImplementation, PlatformFailure> {
        PlatformResolver {
            host,
            execution_profiles: profiles,
            api_versions,
            capabilities,
            implementations,
            pinned_implementation_id: pinned,
        }
        .resolve()
    }

    #[test]
    fn no_cross_os_or_architecture_match_occurs() {
        let host = host(
            OperatingSystem::Macos,
            Architecture::Aarch64,
            DistributionChannel::MacAppStore,
        );
        let profiles = [ExecutionProfile::new("native").unwrap()];
        let api_versions = [HOST_API_V0_3];
        let capabilities = [Capability::SelectionRead];
        let implementations = [
            implementation(
                "org.example.linux",
                90,
                vec![target(
                    OperatingSystem::Linux,
                    Architecture::X86_64,
                    &[DistributionChannel::DesktopDirect],
                )],
            ),
            implementation(
                "org.example.windows",
                80,
                vec![target(
                    OperatingSystem::Windows,
                    Architecture::Aarch64,
                    &[DistributionChannel::DesktopDirect],
                )],
            ),
        ];
        assert_eq!(
            resolve(
                &host,
                &implementations,
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::UnsupportedTarget
        );
    }

    #[test]
    fn multiple_targets_are_alternatives_but_need_matching_channel() {
        let host = host(
            OperatingSystem::Linux,
            Architecture::X86_64,
            DistributionChannel::DesktopDirect,
        );
        let profiles = [ExecutionProfile::new("native").unwrap()];
        let api_versions = [HOST_API_V0_3];
        let capabilities = [Capability::SelectionRead];
        let implementations = [implementation(
            "org.example.multi",
            1,
            vec![
                target(
                    OperatingSystem::Macos,
                    Architecture::Aarch64,
                    &[DistributionChannel::MacAppStore],
                ),
                target(
                    OperatingSystem::Linux,
                    Architecture::X86_64,
                    &[DistributionChannel::DesktopDirect],
                ),
            ],
        )];
        let chosen = resolve(
            &host,
            &implementations,
            &profiles,
            &api_versions,
            &capabilities,
            None,
        )
        .unwrap();
        assert_eq!(chosen.implementation_id, "org.example.multi");
    }

    #[test]
    fn channel_restriction_is_explicit() {
        let host = host(
            OperatingSystem::Macos,
            Architecture::Aarch64,
            DistributionChannel::MacAppStore,
        );
        let profiles = [ExecutionProfile::new("native").unwrap()];
        let api_versions = [HOST_API_V0_3];
        let capabilities = [Capability::SelectionRead];
        let implementations = [implementation(
            "org.example.direct",
            10,
            vec![target(
                OperatingSystem::Macos,
                Architecture::Aarch64,
                &[DistributionChannel::DesktopDirect],
            )],
        )];
        assert_eq!(
            resolve(
                &host,
                &implementations,
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::ChannelRestricted
        );
    }

    #[test]
    fn missing_runtime_profile_or_api_is_reported() {
        let host = host(
            OperatingSystem::Linux,
            Architecture::X86_64,
            DistributionChannel::DesktopDirect,
        );
        let mut candidate = implementation(
            "org.example.native",
            10,
            vec![target(
                OperatingSystem::Linux,
                Architecture::X86_64,
                &[DistributionChannel::DesktopDirect],
            )],
        );
        candidate.api_range = ApiRange {
            major: 0,
            min_minor: 3,
            max_minor: 3,
        };
        let implementations = [candidate];
        let profiles = [ExecutionProfile::new("wasm").unwrap()];
        let api_versions = [HOST_API_V0_3];
        let capabilities = [Capability::SelectionRead];
        assert_eq!(
            resolve(
                &host,
                &implementations,
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::UnsupportedRuntime
        );

        let profiles = [ExecutionProfile::new("native").unwrap()];
        let old_apis = [extension_api::HOST_API];
        assert_eq!(
            resolve(
                &host,
                &implementations,
                &profiles,
                &old_apis,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::UnsupportedRuntime
        );
    }

    #[test]
    fn missing_required_capability_is_reported() {
        let host = host(
            OperatingSystem::Linux,
            Architecture::X86_64,
            DistributionChannel::DesktopDirect,
        );
        let profiles = [ExecutionProfile::new("native").unwrap()];
        let api_versions = [HOST_API_V0_3];
        let capabilities = [Capability::SelectionRead];
        let mut candidate = implementation(
            "org.example.draft",
            10,
            vec![target(
                OperatingSystem::Linux,
                Architecture::X86_64,
                &[DistributionChannel::DesktopDirect],
            )],
        );
        candidate.required_capabilities =
            vec![Capability::SelectionRead, Capability::NoteDraftEdit];
        let implementations = [candidate];
        assert_eq!(
            resolve(
                &host,
                &implementations,
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::MissingCapability
        );
    }

    #[test]
    fn selection_is_priority_descending_then_id_lexicographic() {
        let host = host(
            OperatingSystem::Linux,
            Architecture::X86_64,
            DistributionChannel::DesktopDirect,
        );
        let profiles = [ExecutionProfile::new("native").unwrap()];
        let api_versions = [HOST_API_V0_3];
        let capabilities = [Capability::SelectionRead];
        let target = target(
            OperatingSystem::Linux,
            Architecture::X86_64,
            &[DistributionChannel::DesktopDirect],
        );
        let implementations = [
            implementation("org.example.zeta", 10, vec![target.clone()]),
            implementation("org.example.alpha", 10, vec![target.clone()]),
            implementation("org.example.middle", 20, vec![target.clone()]),
        ];
        assert_eq!(
            resolve(
                &host,
                &implementations,
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap()
            .implementation_id,
            "org.example.middle"
        );

        let implementations = &implementations[..2];
        assert_eq!(
            resolve(
                &host,
                implementations,
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap()
            .implementation_id,
            "org.example.alpha"
        );
    }

    #[test]
    fn pinned_choice_never_falls_back_silently() {
        let host = host(
            OperatingSystem::Linux,
            Architecture::X86_64,
            DistributionChannel::DesktopDirect,
        );
        let profiles = [ExecutionProfile::new("native").unwrap()];
        let api_versions = [HOST_API_V0_3];
        let capabilities = [Capability::SelectionRead];
        let linux_target = target(
            OperatingSystem::Linux,
            Architecture::X86_64,
            &[DistributionChannel::DesktopDirect],
        );
        let implementations = [
            implementation("org.example.high", 100, vec![linux_target.clone()]),
            implementation("org.example.pinned", 1, vec![linux_target]),
            implementation(
                "org.example.other_os",
                90,
                vec![target(
                    OperatingSystem::Web,
                    Architecture::Wasm32,
                    &[DistributionChannel::Web],
                )],
            ),
        ];

        let chosen = resolve(
            &host,
            &implementations,
            &profiles,
            &api_versions,
            &capabilities,
            Some("org.example.pinned"),
        )
        .unwrap();
        assert_eq!(chosen.implementation_id, "org.example.pinned");

        assert_eq!(
            resolve(
                &host,
                &implementations,
                &profiles,
                &api_versions,
                &capabilities,
                Some("org.example.missing"),
            )
            .unwrap_err(),
            PlatformFailure::PinnedUnavailable
        );
        assert_eq!(
            resolve(
                &host,
                &implementations,
                &profiles,
                &api_versions,
                &capabilities,
                Some("org.example.other_os"),
            )
            .unwrap_err(),
            PlatformFailure::PinnedUnavailable
        );
        assert_eq!(
            resolve(
                &host,
                &implementations,
                &profiles,
                &api_versions,
                &capabilities,
                Some(""),
            )
            .unwrap_err(),
            PlatformFailure::PinnedUnavailable
        );
    }

    #[test]
    fn malformed_and_empty_descriptor_sets_are_rejected() {
        let host = host(
            OperatingSystem::Linux,
            Architecture::X86_64,
            DistributionChannel::DesktopDirect,
        );
        let profiles = [ExecutionProfile::new("native").unwrap()];
        let api_versions = [HOST_API_V0_3];
        let capabilities = [Capability::SelectionRead];

        assert_eq!(
            resolve(&host, &[], &profiles, &api_versions, &capabilities, None,).unwrap_err(),
            PlatformFailure::InvalidDescriptor
        );

        let mut empty_targets = implementation("org.example.empty", 1, vec![]);
        assert_eq!(
            resolve(
                &host,
                std::slice::from_ref(&empty_targets),
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::InvalidDescriptor
        );

        let empty_channels = implementation(
            "org.example.channels",
            1,
            vec![target(OperatingSystem::Linux, Architecture::X86_64, &[])],
        );
        assert_eq!(
            resolve(
                &host,
                std::slice::from_ref(&empty_channels),
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::InvalidDescriptor
        );

        let duplicate_id = implementation(
            "org.example.empty",
            2,
            vec![target(
                OperatingSystem::Linux,
                Architecture::X86_64,
                &[DistributionChannel::DesktopDirect],
            )],
        );
        empty_targets.targets = duplicate_id.targets.clone();
        assert_eq!(
            resolve(
                &host,
                &[empty_targets, duplicate_id],
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::InvalidDescriptor
        );

        let mut non_ascii = implementation(
            "org.example.non_ascii",
            1,
            vec![target(
                OperatingSystem::Linux,
                Architecture::X86_64,
                &[DistributionChannel::DesktopDirect],
            )],
        );
        non_ascii.implementation_id = "org.example.阅读".into();
        assert_eq!(
            resolve(
                &host,
                &[non_ascii],
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::InvalidDescriptor
        );

        let too_many: Vec<_> = (0..=64)
            .map(|index| {
                implementation(
                    &format!("org.example.impl{index}"),
                    1,
                    vec![target(
                        OperatingSystem::Linux,
                        Architecture::X86_64,
                        &[DistributionChannel::DesktopDirect],
                    )],
                )
            })
            .collect();
        assert_eq!(
            resolve(
                &host,
                &too_many,
                &profiles,
                &api_versions,
                &capabilities,
                None,
            )
            .unwrap_err(),
            PlatformFailure::InvalidDescriptor
        );
    }
}
