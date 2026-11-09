//! Darwin observations are not a capability to signal a recovered PID.
//! Only the unreaped Child owned by this controller can be terminated. A
//! persisted orphan remains quarantined rather than falling back to kill(pid).
use crate::{CleanupOutcome, ProcessIdentity};
use libproc::{
    bsd_info::BSDInfo,
    proc_pid::{pidinfo, pidpath},
};
use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    process::{Child, Command},
    sync::OnceLock,
    time::Duration,
};

fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}
fn run_id(run: &str) -> io::Result<()> {
    if run.is_empty()
        || run.len() > 96
        || !run
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid("invalid worker run identity"));
    }
    Ok(())
}
fn bsd(pid: u32) -> io::Result<BSDInfo> {
    let pid = i32::try_from(pid).map_err(invalid)?;
    if pid <= 0 {
        return Err(invalid("invalid process PID"));
    }
    pidinfo::<BSDInfo>(pid, 0).map_err(invalid)
}
fn boot_id() -> io::Result<String> {
    // A process cannot survive an OS reboot. Cache this immutable kernel value
    // rather than spawning a utility on every parent-watch observation.
    static BOOT: OnceLock<String> = OnceLock::new();
    if let Some(value) = BOOT.get() {
        return Ok(value.clone());
    }
    // Fixed system utility; no PATH lookup and no inherited environment or data.
    let result = Command::new("/usr/sbin/sysctl")
        .env_clear()
        .args(["-n", "kern.bootsessionuuid"])
        .output()?;
    let value = String::from_utf8(result.stdout)
        .map_err(invalid)?
        .trim()
        .to_ascii_lowercase();
    if !result.status.success()
        || value.len() != 36
        || !value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err(invalid("kernel boot identity unavailable"));
    }
    let _ = BOOT.set(value.clone());
    Ok(value)
}
fn observe(pid: u32, run: String) -> io::Result<ProcessIdentity> {
    let first = bsd(pid)?;
    let image = fs::metadata(pidpath(i32::try_from(pid).map_err(invalid)?).map_err(invalid)?)?;
    let second = bsd(pid)?;
    if first.pbi_pid != pid
        || first.pbi_uid != first.pbi_ruid
        || first.pbi_uid != first.pbi_svuid
        || first.pbi_start_tvsec != second.pbi_start_tvsec
        || first.pbi_start_tvusec != second.pbi_start_tvusec
        || first.pbi_uid != second.pbi_uid
    {
        return Err(invalid("process changed during observation"));
    }
    let start = first
        .pbi_start_tvsec
        .checked_mul(1_000_000)
        .and_then(|n| n.checked_add(first.pbi_start_tvusec))
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("invalid process start identity"))?;
    Ok(ProcessIdentity {
        pid,
        boot_id: boot_id()?,
        start_time_ticks: start,
        uid: first.pbi_uid,
        exe_device: image.dev(),
        exe_inode: image.ino(),
        run_id: run,
    })
}
pub fn current() -> io::Result<ProcessIdentity> {
    observe(std::process::id(), String::new())
}

pub fn inspect(pid: u32, run: &str) -> io::Result<ProcessIdentity> {
    run_id(run)?;
    let observed = observe(pid, run.into())?;
    let own = current()?;
    if observed.uid != own.uid
        || observed.exe_device != own.exe_device
        || observed.exe_inode != own.exe_inode
    {
        return Err(invalid("managed executable identity mismatch"));
    }
    let result = Command::new("/bin/ps")
        .env_clear()
        .args(["-ww", "-p", &pid.to_string(), "-o", "command="])
        .output()?;
    let command = std::str::from_utf8(&result.stdout).map_err(invalid)?;
    let tail = command.split_whitespace().rev().take(4).collect::<Vec<_>>();
    if !result.status.success()
        || result.stdout.len() > 4096
        || tail.len() != 4
        || tail[0] != run
        || tail[3] != "--managed-worker"
        || !tail[1]
            .parse::<u64>()
            .is_ok_and(|n| n > 0 && n.to_string() == tail[1])
        || !tail[2]
            .parse::<u32>()
            .is_ok_and(|n| n > 0 && n.to_string() == tail[2])
    {
        return Err(invalid("managed worker command mismatch"));
    }
    if observe(pid, run.into())? != observed {
        return Err(invalid("worker changed during command observation"));
    }
    // This record alone is never accepted as permission to signal a PID.
    Ok(observed)
}

pub fn guard_worker(parent_pid: u32, start: u64, run: &str) -> io::Result<()> {
    run_id(run)?;
    let parent = observe(parent_pid, String::new())?;
    let own = current()?;
    if start == 0
        || parent.start_time_ticks != start
        || parent.uid != own.uid
        || parent.exe_device != own.exe_device
        || parent.exe_inode != own.exe_inode
        || bsd(own.pid)?.pbi_ppid != parent_pid
    {
        return Err(invalid("worker parent identity mismatch"));
    }
    // Darwin has no Linux prctl parent-death signal. This trusted watcher exits
    // its OWN process on reparenting or loss of the exact parent observation.
    // It never signals a looked-up PID. Polling latency is qualified separately.
    std::thread::Builder::new()
        .name("qusor-parent-guard".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(20));
                let live = bsd(std::process::id()).is_ok_and(|b| b.pbi_ppid == parent_pid)
                    && observe(parent_pid, String::new()).is_ok_and(|p| p == parent);
                if !live {
                    std::process::exit(70);
                }
            }
        })?;
    Ok(())
}

/// Observations of recovered workers never authorize a raw-PID signal.
pub fn cleanup(expected: &ProcessIdentity) -> io::Result<CleanupOutcome> {
    run_id(&expected.run_id)?;
    match observe(expected.pid, expected.run_id.clone()) {
        Ok(actual) if actual != *expected => Ok(CleanupOutcome::Mismatch),
        Ok(_) => Ok(CleanupOutcome::Uncertain),
        Err(error) => Err(error),
    }
}

/// The owning thread retains Child through kill AND try_wait. On Darwin an
/// unreaped owned child cannot have its PID reused; on completion try_wait is
/// the only evidence used to release the durable run lease.
pub fn cleanup_owned(child: &mut Child, expected: &ProcessIdentity) -> io::Result<CleanupOutcome> {
    if child.id() != expected.pid {
        return Ok(CleanupOutcome::Mismatch);
    }
    if child.try_wait()?.is_some() {
        return Ok(CleanupOutcome::Terminated);
    }
    if inspect(child.id(), &expected.run_id)? != *expected {
        return Ok(CleanupOutcome::Mismatch);
    }
    child.kill()?;
    Ok(if child.try_wait()?.is_some() {
        CleanupOutcome::Terminated
    } else {
        CleanupOutcome::Uncertain
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_identity_is_stable_and_nonworker_is_rejected() {
        let identity = current().unwrap();
        assert_eq!(identity, current().unwrap());
        assert!(identity.start_time_ticks > 0);
        assert!(inspect(identity.pid, "forged-run").is_err());
        assert!(guard_worker(identity.pid, identity.start_time_ticks, "forged-run").is_err());
    }
    #[test]
    fn recovered_record_never_authorizes_a_pid_signal() {
        let mut identity = current().unwrap();
        identity.run_id = "forged-run".into();
        assert_eq!(cleanup(&identity).unwrap(), CleanupOutcome::Uncertain);
        identity.start_time_ticks += 1;
        assert_eq!(cleanup(&identity).unwrap(), CleanupOutcome::Mismatch);
    }
}
