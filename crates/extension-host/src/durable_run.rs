//! Pure execution lifecycle gates. OS observations and durable leases stay outside.
use super::{AuthoritySource, ComponentAuthoritySource, DurableHost, DurableSession};
use crate::component::MountHandle;
use extension_api::{ApiError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    Prepared,
    BootReleased,
    ResultAccepted,
    Quarantined,
    Finished,
}

/// One lifecycle for ordinary sessions and temporary component mounts.
/// The journal owns exclusion; only the actual Controller can prove Child reap.
/// Authority handles remain local and never become persistable execution permits.
pub struct ManagedRun<A = DurableSession> {
    authority: A,
    run_id: String,
    phase: RunPhase,
    stopped: bool,
}
fn wrong_phase() -> ApiError {
    ApiError::new(
        ErrorCode::InvalidState,
        "execution lifecycle phase mismatch",
    )
}
impl ManagedRun<DurableSession> {
    pub fn prepare(
        host: &mut DurableHost,
        source: &dyn AuthoritySource,
        session: &DurableSession,
        now: u64,
        run_id: impl Into<String>,
    ) -> Result<Self, ApiError> {
        let run = Self::prepared(session.clone(), run_id.into())?;
        host.validate_current(source, session, now, None)?;
        Ok(run)
    }
    pub fn session(&self) -> &DurableSession {
        &self.authority
    }
    /// GuardReady and durable identity binding must precede this immediate Boot gate.
    pub fn release_boot(
        &mut self,
        host: &mut DurableHost,
        source: &dyn AuthoritySource,
        now: u64,
        send: impl FnOnce() -> Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        self.release_with(|session, run| {
            host.validate_current(source, session, now, Some(run))?;
            send()
        })
    }
    pub fn accept_result<T>(
        &mut self,
        host: &mut DurableHost,
        source: &dyn AuthoritySource,
        now: u64,
        accept: impl FnOnce() -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        self.accept_with(|session, run| {
            host.validate_current(source, session, now, Some(run))?;
            accept()
        })
    }
}
impl ManagedRun<MountHandle> {
    pub fn prepare_component(
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        mount: &MountHandle,
        now: u64,
        run_id: impl Into<String>,
    ) -> Result<Self, ApiError> {
        let run = Self::prepared(mount.clone(), run_id.into())?;
        host.validate_component_current(source, mount, now, None)?;
        Ok(run)
    }
    pub fn mount(&self) -> &MountHandle {
        &self.authority
    }
    pub fn release_component_boot(
        &mut self,
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        now: u64,
        send: impl FnOnce() -> Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        self.release_with(|mount, run| {
            host.validate_component_current(source, mount, now, Some(run))?;
            send()
        })
    }
    pub fn accept_component_result<T>(
        &mut self,
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        now: u64,
        accept: impl FnOnce() -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        self.accept_with(|mount, run| {
            host.validate_component_current(source, mount, now, Some(run))?;
            accept()
        })
    }
}
impl<A> ManagedRun<A> {
    fn prepared(authority: A, run_id: String) -> Result<Self, ApiError> {
        if run_id.is_empty()
            || run_id.len() > 96
            || !run_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "invalid execution run identity",
            ));
        }
        Ok(Self {
            authority,
            run_id,
            phase: RunPhase::Prepared,
            stopped: false,
        })
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn phase(&self) -> RunPhase {
        self.phase
    }
    // Private, so a caller cannot substitute a fake authority check.
    fn release_with(
        &mut self,
        action: impl FnOnce(&A, &str) -> Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        if self.phase != RunPhase::Prepared || self.stopped {
            return Err(wrong_phase());
        }
        let result = action(&self.authority, &self.run_id);
        self.phase = if result.is_ok() {
            RunPhase::BootReleased
        } else {
            RunPhase::Quarantined
        };
        result
    }
    fn accept_with<T>(
        &mut self,
        action: impl FnOnce(&A, &str) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        if self.phase != RunPhase::BootReleased || self.stopped {
            return Err(wrong_phase());
        }
        let result = action(&self.authority, &self.run_id);
        self.phase = if result.is_ok() {
            RunPhase::ResultAccepted
        } else {
            RunPhase::Quarantined
        };
        result
    }
    /// Only the trusted controller's actual owned-Child reap event may call this.
    /// A received result, sent signal or pidfd Terminated observation is not reap.
    pub fn record_reaped(&mut self) -> Result<(), ApiError> {
        if self.phase == RunPhase::Finished || self.stopped {
            return Err(wrong_phase());
        }
        self.stopped = true;
        Ok(())
    }

    /// Only a definite OS spawn failure or an intent never attempted may use
    /// this path. Missing GuardReady or unknown spawn identity is not proof.
    pub fn record_not_spawned(&mut self) -> Result<(), ApiError> {
        if self.phase != RunPhase::Prepared || self.stopped {
            return Err(wrong_phase());
        }
        self.stopped = true;
        self.phase = RunPhase::Quarantined;
        Ok(())
    }

    pub fn quarantine(&mut self) -> Result<(), ApiError> {
        if self.phase == RunPhase::Finished {
            return Err(wrong_phase());
        }
        self.phase = RunPhase::Quarantined;
        Ok(())
    }

    /// Release durable exclusion after confirmed reap or definite non-spawn. Cleanup failure remains
    /// quarantined and may be retried; dropping this helper never clears a lease.
    pub fn finish(
        &mut self,
        clear_lease: impl FnOnce() -> Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        if !self.stopped || self.phase == RunPhase::Finished {
            return Err(wrong_phase());
        }
        match clear_lease() {
            Ok(()) => {
                self.phase = RunPhase::Finished;
                Ok(())
            }
            Err(error) => {
                self.phase = RunPhase::Quarantined;
                Err(error)
            }
        }
    }
}
