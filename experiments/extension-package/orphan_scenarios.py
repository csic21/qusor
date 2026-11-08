"""Linux supervisor-crash validation using independently authored signed packages.

Only this test process becomes a subreaper. Production identity/signal handling
uses safe Rust pidfds; this harness reaps only exact children it created/adopted.
"""
from __future__ import annotations

import ctypes
import hashlib
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import time

from supervisor_scenarios import LiveSupervisor


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def process_state(pid):
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return fields[0], int(fields[19])
    except FileNotFoundError:
        return None


class Subreaper:
    """Reversible test-only PR_SET_CHILD_SUBREAPER; never used by the worker."""
    def __enter__(self):
        self.native = ctypes.CDLL(None, use_errno=True)
        self.native.prctl.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_ulong,
                                      ctypes.c_ulong, ctypes.c_ulong]
        self.native.prctl.restype = ctypes.c_int
        old = ctypes.c_int()
        if self.native.prctl(37, ctypes.byref(old), 0, 0, 0) != 0:
            raise OSError(ctypes.get_errno(), "cannot inspect test subreaper")
        self.old = old.value
        if self.native.prctl(36, ctypes.c_void_p(1), 0, 0, 0) != 0:
            raise OSError(ctypes.get_errno(), "cannot enable test subreaper")
        self.owned = set()
        self.parents = []
        return self

    def track(self, pid):
        if pid is not None:
            self.owned.add(pid)

    def reap(self, pid, timeout=3):
        deadline = time.monotonic() + timeout
        while True:
            child, status = os.waitpid(pid, os.WNOHANG)
            if child == pid:
                self.owned.discard(pid)
                return status
            require(time.monotonic() < deadline, "test child was not confirmed exited")
            time.sleep(0.005)

    def __exit__(self, exc_type, exc_value, traceback):
        # This is failure cleanup, not success evidence. An unreaped owned child
        # cannot have its PID reused; do not scan or signal unrelated processes.
        for parent in self.parents:
            if parent.poll() is None:
                parent.kill()
            parent.wait(timeout=3)
        for pid in self.owned:
            try:
                child, _ = os.waitpid(pid, os.WNOHANG)
                if child == 0:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
            except ChildProcessError:
                pass
        if self.native.prctl(36, ctypes.c_void_p(self.old), 0, 0, 0) != 0:
            raise OSError(ctypes.get_errno(), "cannot restore test subreaper")


def validate_orphans(builder, fixtures, outside, runner, key, report, command, host_action, digest):
    scope = ("synthetic-library", "synthetic-document", "version-1")
    expiry = int(time.time()) + 3600
    packages = {}
    identities = {name: "org.example.orphan-" + name for name in ("a", "b")}
    for name, package_id in identities.items():
        manifest = json.loads((fixtures / "manifest.json").read_text(encoding="utf-8"))
        manifest.update(package_id=package_id, required_capabilities=["document.metadata.read"],
                        optional_capabilities=[], contributions=[])
        manifest_path = outside / f"orphan-{name}-manifest.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        packages[name] = outside / f"orphan-{name}.qpkg"
        command([builder, fixtures / "metadata.wat", manifest_path, fixtures / "asset.txt",
                 fixtures / "test-seed.hex", packages[name]], cwd=outside)
    manifest["package_version"] = {"major": 2, "minor": 0, "patch": 0}
    manifest_path = outside / "orphan-b-update-manifest.json"
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
    packages["b-update"] = outside / "orphan-b-update.qpkg"
    command([builder, fixtures / "metadata.wat", manifest_path, fixtures / "asset.txt",
             fixtures / "test-seed.hex", packages["b-update"]], cwd=outside)
    report["orphan_package_sha256"] = {name: digest(path) for name, path in packages.items()}
    evidence = {"passed": False, "subreaper_is_test_only": True, "cases": []}
    report["supervisor_crash_recovery"] = evidence
    scratch = outside / "orphan-stores"
    scratch.mkdir(mode=0o700)

    def action(name, *args, **kwargs):
        return host_action(runner, "orphan recovery: " + name, args, **kwargs)

    def prepare(name, b_expires=None):
        store = scratch / name
        receipts = {}
        for fixture, package_id in identities.items():
            installed = action(name + " install " + fixture, "install", store,
                               "install-" + fixture, 0, packages[fixture], key)["receipt"]
            receipts[fixture] = action(name + " grant " + fixture, "grant", store,
                                       "grant-" + fixture, installed["revision"], package_id,
                                       *scope, b_expires if fixture == "b" and b_expires is not None else expiry)["receipt"]
        return store, receipts

    def start(live, name, *, ok=True, **fields):
        return live.command("start", ok=ok, package_id=identities[name], library=scope[0],
                            document=scope[1], version=scope[2], **fields)

    def completed(live, name):
        ack = start(live, name)
        event = live.event(ack["run_id"])
        require(event["event"] == "completed" and event["result"]["calls"] == 1,
                "fresh authorized call did not complete after recovery")
        require(json.loads(event["result"]["output"]).get("synthetic") is True,
                "recovered worker did not call the parent broker")
        return ack

    def ready_worker(live, ack):
        deadline = time.monotonic() + 3
        while True:
            state = live.command("status")["status"]
            active = next((row for row in state["active"] if row["run_id"] == ack["run_id"]), None)
            require(active is not None, "worker ended before the crash injection")
            if active["transport_ready"]:
                return state
            require(time.monotonic() < deadline, "worker never confirmed Boot receipt")
            time.sleep(0.002)

    def check_no_lease_bypass(name, store, receipts):
        for operation, args in (
            ("resume", ("resume", store, identities["b"], *scope)),
            ("execute", ("execute", store, identities["b"], *scope)),
            ("install", ("install", store, "bypass-update", receipts["b"]["revision"], packages["b-update"], key)),
            ("grant", ("grant", store, "bypass-grant", receipts["b"]["revision"], identities["b"], *scope, expiry)),
            ("stop", ("disable", store, "bypass-disable", receipts["b"]["revision"], identities["b"])),
            ("uninstall", ("uninstall", store, "bypass-remove", receipts["b"]["revision"], identities["b"])),
        ):
            rejected = action(name + " lease blocks " + operation, *args, ok=False)
            require("lease" in rejected["error"], "ordinary command rejected for an unrelated reason")

    with Subreaper() as reaper:
        def client(store, messages, **fields):
            live = LiveSupervisor(runner, store, messages, **fields)
            reaper.parents.append(live.process)
            return live

        for point in ("before_lease", "after_lease", "after_spawn_intent", "after_spawn", "after_bind", "after_boot"):
            store, receipts = prepare(point)
            case = {"name": point, "messages": []}
            evidence["cases"].append(case)
            live = client(store, case["messages"])
            live.send("start", package_id=identities["b"], library=scope[0], document=scope[1],
                      version=scope[2], fault="kill_failure", crash_point=point)
            checkpoint = live.wait(lambda message: message.get("event") == "launch_checkpoint" and
                                   message.get("point") == point)
            reaper.track(checkpoint.get("pid"))
            live.finish_crashed(86)
            case["checkpoint"] = checkpoint
            if point != "before_lease":
                check_no_lease_bypass(point, store, receipts)
            isolated = point in ("after_spawn_intent", "after_spawn")
            with client(store, case["messages"], clean_shutdown=not isolated) as restarted:
                state = restarted.command("status")["status"]
                require(not state["active"], "restart automatically replayed an old task")
                if isolated:
                    require(state["orphaned"].get(identities["b"]) == "unbound_spawn_uncertain",
                            "unknown PID window was silently cleared")
                    require(not restarted.command("recover", package_id=identities["b"])["recovered"],
                            "unknown PID recovery guessed a process")
                    start(restarted, "b", ok=False)
                    case["outcome"] = "isolated_unbound_spawn_uncertain"
                else:
                    completed(restarted, "b")
                    case["outcome"] = "fresh_session_after_coordination"
                completed(restarted, "a")
            if checkpoint.get("pid"):
                case["test_child_wait_status"] = reaper.reap(checkpoint["pid"])

        for operation in ("cancel", "revoke"):
            store, receipts = prepare("starting-" + operation)
            case = {"name": "starting-" + operation, "messages": []}
            evidence["cases"].append(case)
            with client(store, case["messages"]) as live:
                ack = start(live, "b", fault="hold_guard")
                deadline = time.monotonic() + 3
                while True:
                    state = live.command("status")["status"]
                    active = next(row for row in state["active"] if row["run_id"] == ack["run_id"])
                    if active["guard_ready"]:
                        break
                    require(time.monotonic() < deadline, "test guard was never held")
                    time.sleep(0.002)
                require(not active["identity_bound"] and not active["transport_ready"],
                        "starting-control test already delivered Boot")
                lease = next(row for row in state["worker_leases"] if row["run_id"] == ack["run_id"])
                require(lease["spawn_attempted"] and lease["worker"] is None,
                        "test did not exercise the unbound launch control path")
                if operation == "cancel":
                    live.command("cancel", run_id=ack["run_id"])
                else:
                    stop = live.command("revoke", package_id=identities["b"], operation_id="starting-revoke",
                                        expected_revision=receipts["b"]["revision"])["receipt"]
                    require(stop["generation"] > receipts["b"]["generation"], "starting revoke did not persist")
                ended = live.event(ack["run_id"])
                require(ended["event"] == "failed" and ended["code"] ==
                        ("cancelled" if operation == "cancel" else "revoked"),
                        "starting worker did not honor control before Boot")
                require(not live.command("status")["status"]["worker_leases"],
                        "known terminated starting child left an execution lease")
                if operation == "cancel":
                    completed(live, "b")
                else:
                    start(live, "b", ok=False)
                completed(live, "a")
            case["outcome"] = "starting_authority_closed_before_boot"
            if operation == "revoke":
                action("starting revocation remains durable", "resume", store, identities["b"], *scope, ok=False)

        for expire in (False, True):
            short_expiry = int(time.time()) + 2 if expire else expiry
            store, _ = prepare("guard-release-" + str(expire), b_expires=short_expiry)
            case = {"name": "expired-guard-release" if expire else "valid-guard-release", "messages": []}
            evidence["cases"].append(case)
            with client(store, case["messages"]) as live:
                ack = start(live, "b", fault="hold_guard")
                deadline = time.monotonic() + 3
                while True:
                    state = live.command("status")["status"]
                    active = next(row for row in state["active"] if row["run_id"] == ack["run_id"])
                    if active["guard_ready"]:
                        break
                    require(time.monotonic() < deadline, "guard was not held for authorization check")
                    time.sleep(0.002)
                require(not active["identity_bound"] and not active["boot_queued"] and
                        not active["transport_ready"], "guest module was sent before the test barrier")
                if expire:
                    while int(time.time()) < short_expiry:
                        time.sleep(0.01)
                released = live.command("release_guard", run_id=ack["run_id"])
                require(released["released"] == (not expire), "guard release ignored current authorization")
                ended = live.event(ack["run_id"])
                if expire:
                    require(ended["event"] == "failed" and ended["code"] == "stale_authority" and
                            ended["boot_queued"] is False,
                            "expired authority was rejected only after executable bytes were sent")
                    case["outcome"] = "expired_before_boot_queue"
                else:
                    require(ended["event"] == "completed" and ended["boot_queued"] is True and
                            ended["result"]["calls"] == 1,
                            "valid guard release did not execute the ordinary signed module")
                    case["outcome"] = "valid_barrier_release_runs_module"
                completed(live, "a")

        for survivor in (False, True):
            name = "live_survivor_pidfd" if survivor else "parent_death_guard"
            store, receipts = prepare(name)
            case = {"name": name, "messages": []}
            evidence["cases"].append(case)
            live = client(store, case["messages"])
            ack = start(live, "b", fault="orphan_survivor" if survivor else "kill_failure")
            reaper.track(ack["pid"])
            ready_worker(live, ack)
            require(process_state(ack["pid"])[0] != "Z", "worker not live before parent SIGKILL")
            live.process.kill()
            live.finish_crashed(-signal.SIGKILL)
            status = None
            if survivor:
                require(process_state(ack["pid"])[0] != "Z", "live orphan path was replaced by already-dead confirmation")
                case["live_after_parent_crash"] = True
            else:
                # Confirm the death guard before a new host can send any cleanup signal.
                status = reaper.reap(ack["pid"])
                require(os.WIFSIGNALED(status) and os.WTERMSIG(status) == signal.SIGKILL,
                        "parent-death guard did not terminate the worker independently")
                case["guard_exit_confirmed_before_recovery"] = True
            with client(store, case["messages"]) as restarted:
                state = restarted.command("status")["status"]
                require(not state["active"] and not state["worker_leases"] and not state["orphaned"],
                        "bound old worker was not fully coordinated before accepting a new task")
                fresh = completed(restarted, "b")
                require(fresh["run_id"] != ack["run_id"], "restart reused an old runtime identity")
                completed(restarted, "a")
            if survivor:
                status = reaper.reap(ack["pid"])
            require(os.WIFSIGNALED(status) and os.WTERMSIG(status) == signal.SIGKILL,
                    "old worker was not terminated by the expected guard/pidfd signal")
            case["test_child_wait_status"] = status
            case["outcome"] = "actual_live_pidfd_cleanup" if survivor else "parent_death_signal_confirmed"

        store, receipts = prepare("revoke_commit")
        case = {"name": "revoke_commit", "messages": []}
        evidence["cases"].append(case)
        live = client(store, case["messages"])
        ack = start(live, "b", fault="orphan_survivor")
        reaper.track(ack["pid"])
        ready_worker(live, ack)
        live.send("revoke", package_id=identities["b"], operation_id="durable-revoke",
                  expected_revision=receipts["b"]["revision"], crash_point="after_revoke_commit")
        live.wait(lambda message: message.get("event") == "launch_checkpoint" and
                  message.get("point") == "after_revoke_commit")
        live.finish_crashed(86)
        with client(store, case["messages"]) as restarted:
            state = restarted.command("status")["status"]
            require(not state["worker_leases"], "fenced lease failed to recover after committed revoke")
            current = next(row for row in state["installations"] if row["package_id"] == identities["b"])
            require(current["generation"] > receipts["b"]["generation"], "crash revived old authorization generation")
            start(restarted, "b", ok=False)
            completed(restarted, "a")
        case["test_child_wait_status"] = reaper.reap(ack["pid"])
        action("revocation remains durable after recovered supervisor exits", "resume", store,
               identities["b"], *scope, ok=False)
        case["outcome"] = "revoked_fenced_lease_coordinated"

        store, receipts = prepare("foreign_identity")
        case = {"name": "foreign_identity", "messages": []}
        evidence["cases"].append(case)
        live = client(store, case["messages"])
        ack = start(live, "b", fault="orphan_survivor")
        reaper.track(ack["pid"])
        ready_worker(live, ack)
        live.process.kill()
        live.finish_crashed(-signal.SIGKILL)
        with sqlite3.connect(store / "state.sqlite3") as db:
            original, original_hash = db.execute("SELECT record,record_hash FROM worker_leases").fetchone()
        damaged = json.loads(original)
        boot = damaged["owner"]["boot_id"]
        damaged["owner"]["boot_id"] = ("0" if boot[0] != "0" else "1") + boot[1:]
        with sqlite3.connect(store / "state.sqlite3") as db:
            db.execute("UPDATE worker_leases SET record=?",
                       (json.dumps(damaged, separators=(",", ":")),))
        rejected = action("valid-UUID bitrot cannot masquerade as a kernel restart", "status", store, ok=False)
        require("integrity" in rejected["error"], "lease corruption failed at an unrelated boundary")
        require(process_state(ack["pid"])[0] != "Z", "corrupt record triggered process cleanup")
        case["valid_uuid_bitrot_rejected_before_cleanup"] = True
        with sqlite3.connect(store / "state.sqlite3") as db:
            db.execute("UPDATE worker_leases SET record=?,record_hash=?", (original, original_hash))
        foreign = subprocess.Popen(["sleep", "60"], stdin=subprocess.DEVNULL,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            record = json.loads(original)
            record["worker"]["pid"] = foreign.pid
            record["worker"]["start_time_ticks"] = process_state(foreign.pid)[1]
            mutated = json.dumps(record, ensure_ascii=False, separators=(",", ":"))
            with sqlite3.connect(store / "state.sqlite3") as db:
                db.execute("UPDATE worker_leases SET record=?,record_hash=?",
                           (mutated, hashlib.sha256(mutated.encode()).hexdigest()))
            with client(store, case["messages"], clean_shutdown=False) as restarted:
                state = restarted.command("status")["status"]
                require(state["orphaned"].get(identities["b"]) == "identity_mismatch",
                        "foreign process identity was not isolated")
                require(foreign.poll() is None, "recovery signalled an unrelated live process")
                require(not restarted.command("recover", package_id=identities["b"])["recovered"],
                        "explicit retry cleared an identity mismatch")
                start(restarted, "b", ok=False)
                completed(restarted, "a")
                require(foreign.poll() is None, "A recovery affected the unrelated process")
            with sqlite3.connect(store / "state.sqlite3") as db:
                db.execute("UPDATE worker_leases SET record=?,record_hash=?", (original, original_hash))
            with client(store, case["messages"]) as repaired:
                require(not repaired.command("status")["status"]["worker_leases"], "restored test record failed safe cleanup")
                completed(repaired, "b")
            case["test_child_wait_status"] = reaper.reap(ack["pid"])
            case["foreign_process_remained_live"] = foreign.poll() is None
            case["outcome"] = "mismatch_isolated_without_signalling_foreign_process"
        finally:
            if foreign.poll() is None:
                foreign.kill()
            foreign.wait(timeout=3)
    evidence["passed"] = True
