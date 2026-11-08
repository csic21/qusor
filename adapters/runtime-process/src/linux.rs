//! Linux process identity, parent-death guard and pinned pidfd cleanup.
//!
//! A PID is a lookup key, never authority to signal. Cleanup pins a kernel process
//! object before checking its recorded identity and the managed-worker command.
//! `Terminated` includes a zombie and must never be displayed as `reaped`.
use crate::{CleanupOutcome, ProcessIdentity};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fd::OwnedFd,
    process::{
        Pid, PidfdFlags, Signal, getppid, pidfd_open, pidfd_send_signal,
        set_parent_process_death_signal,
    },
};
use std::{
    fs::{self, File},
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_PROC_BYTES: usize = 64 * 1024;
const MAX_CMDLINE_BYTES: usize = 4096;
const MAX_RUN_ID_BYTES: usize = 96;
const CLEANUP_WAIT: Duration = Duration::from_millis(50);

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn limited(path: &Path, budget: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(budget as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > budget {
        return Err(invalid("process metadata budget exceeded"));
    }
    Ok(bytes)
}
fn proc_path(pid: u32, name: &str) -> PathBuf {
    PathBuf::from(format!("/proc/{pid}/{name}"))
}
fn number<T: std::str::FromStr>(bytes: &[u8]) -> io::Result<T> {
    std::str::from_utf8(bytes)
        .map_err(|_| invalid("invalid process number"))?
        .parse()
        .map_err(|_| invalid("invalid process number"))
}
fn valid_pid(pid: u32) -> io::Result<Pid> {
    let raw = i32::try_from(pid).map_err(|_| invalid("invalid process PID"))?;
    Pid::from_raw(raw).ok_or_else(|| invalid("invalid process PID"))
}
fn validate_run_id(value: &str) -> io::Result<()> {
    if value.is_empty()
        || value.len() > MAX_RUN_ID_BYTES
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid("invalid worker run identity"));
    }
    Ok(())
}
fn boot_id() -> io::Result<String> {
    let bytes = limited(Path::new("/proc/sys/kernel/random/boot_id"), 64)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| invalid("invalid kernel boot identity"))?
        .trim();
    if text.len() != 36
        || !text.bytes().enumerate().all(|(index, value)| {
            if [8, 13, 18, 23].contains(&index) {
                value == b'-'
            } else {
                value.is_ascii_digit() || (b'a'..=b'f').contains(&value)
            }
        })
    {
        return Err(invalid("invalid kernel boot identity"));
    }
    Ok(text.to_owned())
}
fn parse_start_time(bytes: &[u8], expected_pid: u32) -> io::Result<u64> {
    // comm may contain spaces, parentheses and non-UTF8; only its last ')' delimits it.
    let open = bytes
        .iter()
        .position(|b| *b == b'(')
        .ok_or_else(|| invalid("invalid process stat"))?;
    let close = bytes
        .iter()
        .rposition(|b| *b == b')')
        .ok_or_else(|| invalid("invalid process stat"))?;
    if open == 0 || close <= open || number::<u32>(bytes[..open].trim_ascii())? != expected_pid {
        return Err(invalid("process stat identity mismatch"));
    }
    let suffix: Vec<_> = bytes[close + 1..]
        .split(|b| b.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .collect();
    let start = number::<u64>(
        suffix
            .get(19)
            .ok_or_else(|| invalid("incomplete process stat"))?,
    )?;
    if start == 0 {
        return Err(invalid("invalid process start identity"));
    }
    Ok(start)
}
fn start_time(pid: u32) -> io::Result<u64> {
    parse_start_time(&limited(&proc_path(pid, "stat"), MAX_PROC_BYTES)?, pid)
}
fn process_uid(pid: u32) -> io::Result<u32> {
    let bytes = limited(&proc_path(pid, "status"), MAX_PROC_BYTES)?;
    let mut lines = bytes
        .split(|b| *b == b'\n')
        .filter_map(|line| line.strip_prefix(b"Uid:"));
    let line = lines.next().ok_or_else(|| invalid("missing process UID"))?;
    if lines.next().is_some() {
        return Err(invalid("duplicate process UID"));
    }
    let values = line
        .split(|b| b.is_ascii_whitespace())
        .filter(|v| !v.is_empty())
        .map(number::<u32>)
        .collect::<io::Result<Vec<_>>>()?;
    if values.len() != 4 || values.iter().any(|uid| *uid != values[0]) {
        return Err(invalid("unsupported process credentials"));
    }
    Ok(values[0])
}
fn inspect_process(pid: u32, run_id: String) -> io::Result<ProcessIdentity> {
    valid_pid(pid)?;
    let boot_id = boot_id()?;
    let start_time_ticks = start_time(pid)?;
    let uid = process_uid(pid)?;
    let image = fs::metadata(proc_path(pid, "exe"))?;
    let second_image = fs::metadata(proc_path(pid, "exe"))?;
    if start_time(pid)? != start_time_ticks
        || process_uid(pid)? != uid
        || image.dev() != second_image.dev()
        || image.ino() != second_image.ino()
    {
        return Err(invalid("process changed during identity observation"));
    }
    Ok(ProcessIdentity {
        pid,
        boot_id,
        start_time_ticks,
        uid,
        exe_device: image.dev(),
        exe_inode: image.ino(),
        run_id,
    })
}
fn worker_command(pid: u32, run_id: &str) -> io::Result<bool> {
    let bytes = limited(&proc_path(pid, "cmdline"), MAX_CMDLINE_BYTES)?;
    if bytes.last() != Some(&0) {
        return Ok(false);
    }
    let args: Vec<_> = bytes[..bytes.len() - 1].split(|b| *b == 0).collect();
    if args.len() != 5
        || args[0].is_empty()
        || args[1] != b"--managed-worker"
        || args[4] != run_id.as_bytes()
    {
        return Ok(false);
    }
    let parent = number::<u32>(args[2])?;
    let start = number::<u64>(args[3])?;
    Ok(parent != 0
        && start != 0
        && args[2] == parent.to_string().as_bytes()
        && args[3] == start.to_string().as_bytes())
}

pub fn current() -> io::Result<ProcessIdentity> {
    inspect_process(std::process::id(), String::new())
}

/// Capture only a managed worker with the exact nonce-bearing native command.
pub fn inspect(pid: u32, run_id: &str) -> io::Result<ProcessIdentity> {
    validate_run_id(run_id)?;
    let identity = inspect_process(pid, run_id.to_owned())?;
    if !worker_command(pid, run_id)? {
        return Err(invalid("managed worker command mismatch"));
    }
    Ok(identity)
}

/// Install KILL-on-parent-thread-death before reading Boot, then check the parent
/// again. A parent that died before prctl is not retroactively signalled by Linux.
/// Call from the native worker's main thread before starting other work.
pub fn guard_worker(parent_pid: u32, parent_start_time_ticks: u64, run_id: &str) -> io::Result<()> {
    validate_run_id(run_id)?;
    valid_pid(parent_pid)?;
    if parent_start_time_ticks == 0 {
        return Err(invalid("invalid parent start identity"));
    }
    set_parent_process_death_signal(Some(Signal::KILL))?;
    if getppid().map(|pid| pid.as_raw_nonzero().get() as u32) != Some(parent_pid) {
        return Err(invalid("worker parent changed before guard"));
    }
    let own = current()?;
    let parent = inspect_process(parent_pid, String::new())?;
    if parent.start_time_ticks != parent_start_time_ticks
        || own.uid != parent.uid
        || own.exe_device != parent.exe_device
        || own.exe_inode != parent.exe_inode
        || getppid().map(|pid| pid.as_raw_nonzero().get() as u32) != Some(parent_pid)
    {
        return Err(invalid("worker parent identity mismatch"));
    }
    Ok(())
}
fn exited(fd: &OwnedFd, wait: Duration) -> io::Result<bool> {
    let mut descriptors = [PollFd::new(fd, PollFlags::IN)];
    let timeout = Timespec::try_from(wait).map_err(|_| invalid("invalid process poll timeout"))?;
    poll(&mut descriptors, Some(&timeout))?;
    let flags = descriptors[0].revents();
    if flags.intersects(PollFlags::NVAL | PollFlags::ERR) {
        return Err(io::Error::other("process descriptor observation failed"));
    }
    Ok(flags.intersects(PollFlags::IN | PollFlags::HUP))
}

/// Signal only a pinned, revalidated same-image managed worker. No raw-PID kill
/// fallback exists. Unsupported kernel APIs and permissions propagate as errors.
pub fn cleanup(expected: &ProcessIdentity) -> io::Result<CleanupOutcome> {
    validate_run_id(&expected.run_id)?;
    let pid = valid_pid(expected.pid)?;
    if expected.pid <= 1 || expected.pid == std::process::id() {
        return Ok(CleanupOutcome::Mismatch);
    }
    let fd = match pidfd_open(pid, PidfdFlags::NONBLOCK) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::SRCH) => return Ok(CleanupOutcome::Absent),
        Err(error) => return Err(error.into()),
    };
    if exited(&fd, Duration::ZERO)? {
        return Ok(CleanupOutcome::Terminated);
    }
    // Confirm disappearance without sending a signal even when a host upgrade
    // changed its native image. Only a still-live target requires image trust.
    let own = current()?;
    if expected.boot_id != own.boot_id
        || expected.uid != own.uid
        || expected.exe_device != own.exe_device
        || expected.exe_inode != own.exe_inode
    {
        return Ok(CleanupOutcome::Mismatch);
    }
    let observed = inspect(expected.pid, &expected.run_id);
    // The pinned object may have exited (and the numeric PID been recycled)
    // during /proc reads. This check concerns that object, not any replacement.
    if exited(&fd, Duration::ZERO)? {
        return Ok(CleanupOutcome::Terminated);
    }
    match observed {
        Ok(value) if &value == expected => {}
        Ok(_) => return Ok(CleanupOutcome::Mismatch),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::InvalidData
            ) =>
        {
            return Ok(CleanupOutcome::Mismatch);
        }
        Err(error) => return Err(error),
    }
    match pidfd_send_signal(&fd, Signal::KILL) {
        Ok(()) => {}
        Err(rustix::io::Errno::SRCH) => return Ok(CleanupOutcome::Terminated),
        Err(error) => return Err(error.into()),
    }
    if exited(&fd, CLEANUP_WAIT)? {
        Ok(CleanupOutcome::Terminated)
    } else {
        Ok(CleanupOutcome::Uncertain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_parser_does_not_confuse_comm_parentheses_or_spaces_with_fields() {
        let mut text = b"123 (a name ) with ) chars) S".to_vec();
        for _ in 0..18 {
            text.extend_from_slice(b" 0");
        }
        text.extend_from_slice(b" 98765 0\n");
        assert_eq!(parse_start_time(&text, 123).unwrap(), 98765);
        assert!(parse_start_time(&text, 124).is_err());
        assert!(parse_start_time(b"123 (bad) S 1", 123).is_err());
        assert!(parse_start_time(b"123 no delimiters", 123).is_err());
    }

    #[test]
    fn exited_or_absent_target_is_confirmed_before_native_image_compatibility() {
        use std::process::{Child, Command};
        struct OwnedChild(Child);
        impl Drop for OwnedChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = OwnedChild(Command::new("/bin/sleep").arg("30").spawn().unwrap());
        let mut old_image = inspect_process(child.0.id(), "old-native-image-run".into()).unwrap();
        old_image.exe_inode = 1;
        // The live foreign image is still never an eligible signalling target.
        assert_eq!(cleanup(&old_image).unwrap(), CleanupOutcome::Mismatch);
        assert!(child.0.try_wait().unwrap().is_none());
        let fd = pidfd_open(valid_pid(child.0.id()).unwrap(), PidfdFlags::NONBLOCK).unwrap();
        child.0.kill().unwrap();
        assert!(exited(&fd, Duration::from_secs(3)).unwrap());
        // Exited is observable before wait/reap, despite the obsolete image.
        assert_eq!(cleanup(&old_image).unwrap(), CleanupOutcome::Terminated);
        child.0.wait().unwrap();
        assert_eq!(cleanup(&old_image).unwrap(), CleanupOutcome::Absent);
    }

    #[test]
    fn current_identity_has_a_native_image_and_worker_nonce_is_required() {
        let observed = current().unwrap();
        assert_eq!(observed.pid, std::process::id());
        assert!(observed.start_time_ticks > 0 && observed.exe_inode > 0);
        assert!(inspect(observed.pid, "not-a-worker").is_err());
        assert!(cleanup(&observed).is_err());
        let mut not_a_worker = observed;
        not_a_worker.run_id = "identity-test-run".into();
        assert_eq!(cleanup(&not_a_worker).unwrap(), CleanupOutcome::Mismatch);
    }
}
