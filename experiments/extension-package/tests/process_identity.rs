//! Real Linux processes for identity pinning and parent-death guard verification.
//! Helpers re-execute this test binary; none runs guest code or touches a Store.
use qusor_package_probe::process_identity::{CleanupOutcome, cleanup, current, guard_worker};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    process::{
        Pid, PidfdFlags, Signal, WaitOptions, getpid, pidfd_open, pidfd_send_signal,
        set_child_subreaper, waitpid,
    },
};
use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const TEST_NAME: &str = "real_identity_pinning_and_parent_death_guard";
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn helper(role: &str) -> OwnedChild {
    OwnedChild(
        Command::new(env::current_exe().unwrap())
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env_clear()
            .env("QUSOR_IDENTITY_HELPER", role)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    )
}
fn read_number(reader: &mut impl BufRead, prefix: &str) -> u32 {
    let mut line = String::new();
    loop {
        line.clear();
        assert_ne!(
            reader.read_line(&mut line).unwrap(),
            0,
            "helper ended before {prefix}"
        );
        if let Some(index) = line.find(prefix) {
            return line[index + prefix.len()..]
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
        }
    }
}
fn wait_ready(fd: &rustix::fd::OwnedFd) {
    let timeout = Timespec::try_from(Duration::from_secs(3)).unwrap();
    let mut descriptors = [PollFd::new(fd, PollFlags::IN)];
    assert_eq!(poll(&mut descriptors, Some(&timeout)).unwrap(), 1);
    assert!(
        descriptors[0]
            .revents()
            .intersects(PollFlags::IN | PollFlags::HUP)
    );
}

#[test]
fn real_identity_pinning_and_parent_death_guard() {
    match env::var("QUSOR_IDENTITY_HELPER").as_deref() {
        Ok("guarded-child") => {
            let parent: u32 = env::var("QUSOR_EXPECT_PARENT").unwrap().parse().unwrap();
            let start: u64 = env::var("QUSOR_EXPECT_START").unwrap().parse().unwrap();
            guard_worker(parent, start, "identity-test-run").unwrap();
            println!("GUARDED {}", std::process::id());
            std::io::stdout().flush().unwrap();
            thread::sleep(Duration::from_secs(30));
            panic!("parent-death signal did not terminate child");
        }
        Ok("bad-guard") => {
            let parent: u32 = env::var("QUSOR_EXPECT_PARENT").unwrap().parse().unwrap();
            let start: u64 = env::var("QUSOR_EXPECT_START").unwrap().parse().unwrap();
            assert!(guard_worker(parent, start + 1, "identity-test-run").is_err());
            return;
        }
        Ok("parent") => {
            let identity = current().unwrap();
            let mut child = OwnedChild(
                Command::new(env::current_exe().unwrap())
                    .args(["--exact", TEST_NAME, "--nocapture"])
                    .env_clear()
                    .env("QUSOR_IDENTITY_HELPER", "guarded-child")
                    .env("QUSOR_EXPECT_PARENT", identity.pid.to_string())
                    .env("QUSOR_EXPECT_START", identity.start_time_ticks.to_string())
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            let mut output = BufReader::new(child.0.stdout.take().unwrap());
            let pid = read_number(&mut output, "GUARDED ");
            println!("WORKER_ID {pid}");
            std::io::stdout().flush().unwrap();
            let mut byte = [0];
            std::io::stdin().read_exact(&mut byte).unwrap();
            panic!("parent helper expected to be killed by outer test");
        }
        _ => {}
    }

    // A forged record referring to another live native image must not signal it.
    let mut foreign = OwnedChild(Command::new("/bin/sleep").arg("30").spawn().unwrap());
    let mut mismatch = current().unwrap();
    mismatch.pid = foreign.0.id();
    mismatch.run_id = "identity-test-run".into();
    assert_eq!(cleanup(&mismatch).unwrap(), CleanupOutcome::Mismatch);
    assert!(foreign.0.try_wait().unwrap().is_none());

    // Pin the exact child object and verify readiness remains attached to it
    // after exit. No production path falls back to signal(number).
    let pinned = pidfd_open(
        Pid::from_raw(foreign.0.id() as i32).unwrap(),
        PidfdFlags::NONBLOCK,
    )
    .unwrap();
    pidfd_send_signal(&pinned, Signal::KILL).unwrap();
    wait_ready(&pinned);
    foreign.0.wait().unwrap();
    assert!(pidfd_send_signal(&pinned, Signal::KILL).is_err());

    let own = current().unwrap();
    let rejected = Command::new(env::current_exe().unwrap())
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env_clear()
        .env("QUSOR_IDENTITY_HELPER", "bad-guard")
        .env("QUSOR_EXPECT_PARENT", own.pid.to_string())
        .env("QUSOR_EXPECT_START", own.start_time_ticks.to_string())
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(
        rejected.success(),
        "a stale expected parent identity was accepted"
    );

    // Adopt the guarded orphan so this test also reaps it rather than relying
    // on the surrounding container's PID 1 implementation.
    let previous = rustix::process::child_subreaper().unwrap();
    set_child_subreaper(Some(getpid())).unwrap();
    let mut parent = helper("parent");
    let mut output = BufReader::new(parent.0.stdout.take().unwrap());
    let worker_pid = read_number(&mut output, "WORKER_ID ");
    let worker = Pid::from_raw(worker_pid as i32).unwrap();
    let worker_fd = pidfd_open(worker, PidfdFlags::NONBLOCK).unwrap();
    parent.0.kill().unwrap();
    let killed = parent.0.wait().unwrap();
    assert!(!killed.success());
    wait_ready(&worker_fd);
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some((pid, status)) = waitpid(Some(worker), WaitOptions::NOHANG).unwrap() {
            assert_eq!(pid, worker);
            break status;
        }
        assert!(Instant::now() < deadline, "guarded child was not reaped");
        thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(status.terminating_signal(), Some(Signal::KILL.as_raw()));
    set_child_subreaper(previous).unwrap();
}
