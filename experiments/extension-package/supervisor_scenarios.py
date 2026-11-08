"""Live JSONL supervisor validation with independent ordinary signed packages."""
from __future__ import annotations

import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time


class LiveSupervisor:
    """Correlate management acknowledgements without losing asynchronous events."""
    def __init__(self, runner, store, records, *, clean_shutdown=True):
        self.records = records
        self.clean_shutdown = clean_shutdown
        self.process = subprocess.Popen([str(runner), "supervise", str(store)],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True, encoding="utf-8")
        self.messages = queue.Queue(maxsize=2048)
        self.saved = []
        self.sequence = 0
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        try:
            for line in self.process.stdout:
                if len(line) > 131072:
                    raise RuntimeError("supervisor output exceeded validation budget")
                self.messages.put(json.loads(line), timeout=5)
            self.messages.put({"validator_eof": True}, timeout=5)
        except Exception as error:
            self.messages.put({"validator_reader_error": str(error)}, timeout=5)

    def wait(self, predicate, timeout=5):
        for index, message in enumerate(self.saved):
            if predicate(message):
                return self.saved.pop(index)
        deadline = time.monotonic() + timeout
        while True:
            try:
                message = self.messages.get(timeout=max(0.001, deadline - time.monotonic()))
            except queue.Empty as error:
                raise RuntimeError("supervisor did not produce the expected response") from error
            self.records.append({"seconds": time.monotonic(), "message": message})
            if message.get("validator_eof") or message.get("validator_reader_error"):
                raise RuntimeError("supervisor transport ended: " + str(message))
            if predicate(message):
                return message
            self.saved.append(message)
            if time.monotonic() >= deadline:
                raise RuntimeError("supervisor response deadline exceeded")

    def send(self, op, **fields):
        self.sequence += 1
        request_id = f"command-{self.sequence}"
        payload = dict(fields, op=op, request_id=request_id)
        self.records.append({"command": payload, "seconds": time.monotonic()})
        self.process.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n")
        self.process.stdin.flush()
        return request_id

    def command(self, op, *, ok=True, **fields):
        request_id = self.send(op, **fields)
        reply = self.wait(lambda message: message.get("request_id") == request_id)
        if reply.get("ok") != ok:
            raise RuntimeError("unexpected management result: " + str(reply))
        return reply

    def event(self, run_id, *, timeout=5, kinds=("completed", "failed", "quarantined")):
        return self.wait(lambda message: message.get("run_id") == run_id and
                         message.get("event") in kinds, timeout)

    def close(self):
        if self.process.poll() is None:
            try:
                self.command("shutdown")
                completion = self.wait(lambda message: message.get("event") == "shutdown_complete")
                if completion.get("confirmed") is not self.clean_shutdown:
                    raise RuntimeError("supervisor cleanup confirmation disagrees with unresolved state")
                self.process.wait(timeout=5)
            except Exception:
                self.process.kill()
                self.process.wait(timeout=5)
                raise
        else:
            raise RuntimeError("supervisor exited before explicit shutdown")
        if self.process.returncode != (0 if self.clean_shutdown else 1):
            raise RuntimeError("supervisor returned an unexpected cleanup exit: " + self.process.stderr.read())
        self.process.stdin.close()
        self.process.stdout.close()
        self.process.stderr.close()
        self.reader.join(timeout=1)

    def finish_crashed(self, expected):
        """Dispose a deliberately crashed parent without claiming graceful shutdown."""
        self.process.wait(timeout=5)
        if self.process.returncode != expected:
            raise RuntimeError("supervisor did not crash at the requested boundary")
        self.reader.join(timeout=1)
        while True:
            try:
                message = self.messages.get_nowait()
            except queue.Empty:
                break
            self.records.append({"seconds": time.monotonic(), "message": message})
        self.records.append({"crash_exit": self.process.returncode, "pid": self.process.pid})
        for pipe in (self.process.stdin, self.process.stdout, self.process.stderr):
            try:
                pipe.close()
            except BrokenPipeError:
                pass

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        self.close()


def validate_supervisor(builder, fixtures, outside, runner, key, report, command, host_action, digest):
    def require(condition, message):
        if not condition:
            raise RuntimeError(message)

    def action(name, *args, **kwargs):
        return host_action(runner, "supervisor preparation: " + name, args, **kwargs)

    scope = ("synthetic-library", "synthetic-document", "version-1")
    expiry = int(time.time()) + 3600
    store = outside / "supervisor-store"
    manifests = {}
    receipts = {}
    packages = {}
    for name, module in (("a", "metadata.wat"), ("b", "metadata.wat"),
                         ("loop", "infinite-loop.wat"), ("trap", "metadata-trap.wat"),
                         ("denied", "metadata-wrong-document.wat"), ("launch", "metadata.wat"),
                         ("bootstrap", "managed-invalid-entry.wat"),
                         ("eight", "managed-eight-calls.wat"), ("nine", "managed-nine-calls.wat")):
        manifest = json.loads((fixtures / "manifest.json").read_text(encoding="utf-8"))
        package_id = "org.example.supervised-" + name
        manifest.update(package_id=package_id, required_capabilities=["document.metadata.read"],
                        optional_capabilities=[], contributions=[])
        manifest_path = outside / f"supervised-{name}.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        packages[name] = outside / f"supervised-{name}.qpkg"
        command([builder, fixtures / module, manifest_path, fixtures / "asset.txt",
                 fixtures / "test-seed.hex", packages[name]], cwd=outside)
        manifests[name] = package_id
        installed = action(name + " install", "install", store, "install-" + name, 0, packages[name], key)["receipt"]
        receipts[name] = action(name + " grant", "grant", store, "grant-" + name,
                                installed["revision"], package_id, *scope, expiry)["receipt"]
    report["supervisor_package_sha256"] = {name: digest(path) for name, path in packages.items()}
    records = []
    report["persistent_supervisor"] = {"messages": records, "passed": False,
                                        "reaper_faults_are_injected": True}
    checks = []
    worker_start_times = {}

    def process_start(pid):
        try:
            return Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19]
        except FileNotFoundError:
            return None

    with LiveSupervisor(runner, store, records) as live:
        supervisor_pid = live.process.pid
        report["persistent_supervisor"]["pid"] = supervisor_pid

        def start(name, fault=None, *, ok=True):
            payload = {"package_id": manifests[name], "library": scope[0],
                       "document": scope[1], "version": scope[2]}
            if fault:
                payload["fault"] = fault
            reply = live.command("start", ok=ok, **payload)
            if ok:
                identity = process_start(reply["pid"])
                if identity is not None:
                    worker_start_times[str(reply["pid"])] = identity
            return reply

        def completed(ack, calls=1):
            event = live.event(ack["run_id"])
            require(event["event"] == "completed", "normal plugin did not complete: " + str(event))
            result = event["result"]
            metadata = json.loads(result["output"])
            require(result["calls"] == calls and result["fuel_consumed"] > 0 and
                    metadata.get("synthetic") is True and metadata["library"] == scope[0] and
                    metadata["document"] == scope[1] and metadata["version"] == scope[2],
                    "normal plugin did not consume its scoped parent broker response")
            return event

        def pending_broker(ack):
            deadline = time.monotonic() + 3
            while True:
                state = live.command("status")["status"]
                row = next((row for row in state["active"] if row["run_id"] == ack["run_id"]), None)
                require(row is not None, "worker ended before its pending broker call was observed")
                if row["state"] == "running" and row["transport_ready"]:
                    return state
                require(time.monotonic() < deadline, "worker never reached a pending broker call")
                time.sleep(0.002)

        def stream_index(event):
            return next(index for index, record in enumerate(records)
                        if record.get("message") == event)

        blocked = start("b", "block_host_call")
        status = pending_broker(blocked)
        require(any(row["run_id"] == blocked["run_id"] for row in status["active"]),
                "blocked plugin was not confirmed live alongside the manager")
        descriptors = []
        for fd in Path(f"/proc/{blocked['pid']}/fd").iterdir():
            try:
                descriptors.append(os.readlink(fd))
            except FileNotFoundError:
                pass
        require(bool(descriptors), "live worker descriptors were not inspected")
        require(not any(str(store) in value or "state.sqlite3" in value or "writer.lock" in value
                        for value in descriptors), "worker inherited the management store handle")
        report["persistent_supervisor"]["blocked_worker_descriptors"] = descriptors
        healthy = completed(start("a"))
        failure = live.event(blocked["run_id"])
        require(failure["event"] == "failed" and failure["code"] == "watchdog_timeout",
                "blocked host call did not fail at the process watchdog")
        require(stream_index(healthy) < stream_index(failure),
                "healthy plugin only completed after blocked plugin was removed")
        completed(start("a"))
        checks.append("A completes while B host call is pending; manager status responds")

        for name, fault, code in (("loop", None, "fuel_exhausted"),
                                  ("trap", None, "vm_trap"),
                                  ("denied", None, "permission_denied"),
                                  ("b", "crash", "worker_exit")):
            bad = start(name, fault)
            completed(start("a"))
            failed = live.event(bad["run_id"])
            require(failed["event"] == "failed" and failed["code"] == code,
                    "fault did not hit expected boundary: " + str(failed))
            completed(start("a"))
            checks.append(name + " fails without stopping A or the manager")

        completed(start("eight"), calls=8)
        ninth = start("nine")
        failed = live.event(ninth["run_id"])
        require(failed["event"] == "failed" and failed["code"] == "host_call_limit",
                "guest call limit was not enforced at the ninth call")
        completed(start("a"))
        checks.append("eight broker calls succeed, ninth call is rejected without a protocol off-by-one")

        for fault in ("kill_failure", "wait_failure"):
            bad = start("b", fault)
            quarantined = live.event(bad["run_id"])
            require(quarantined["event"] == "quarantined", "uncertain cleanup did not quarantine")
            status = live.command("status")["status"]
            require(any(row["run_id"] == bad["run_id"] for row in status["quarantined"]),
                    "manager lost the unresolved worker handle")
            start("b", ok=False)
            completed(start("a"))
            deadline = time.monotonic() + 3
            while not live.command("reap", package_id=manifests["b"])["reaped"]:
                require(time.monotonic() < deadline, "explicit worker reap did not confirm exit")
                start("b", ok=False)
                time.sleep(0.01)
            completed(start("b"))
            checks.append(fault + " prohibits replacement until explicit confirmed reap")

        delayed = start("b", "delay_result")
        old_failure = live.event(delayed["run_id"])
        require(old_failure["event"] == "failed" and old_failure["code"] == "watchdog_timeout",
                "delayed old result did not outlive its authority")
        replacement = start("b")
        require(replacement["run_id"] != delayed["run_id"], "replacement reused runtime identity")
        completed(replacement)
        stale = live.event(delayed["run_id"], kinds=("stale_event",))
        require(stale["event"] == "stale_event", "old transport result was not rejected")
        completed(start("a"))
        checks.append("actually delayed old result is rejected after a new run completes")

        cancelled = start("b", "block_host_call")
        pending_broker(cancelled)
        live.command("cancel", run_id=cancelled["run_id"])
        failure = live.event(cancelled["run_id"])
        require(failure["event"] == "failed" and failure["code"] == "cancelled",
                "cancel did not close execution authority")
        completed(start("b"))
        checks.append("cancellation closes the old run, explicit retry uses a fresh session")

        revoked = start("b", "block_host_call")
        pending_broker(revoked)
        stop = live.command("revoke", package_id=manifests["b"], operation_id="supervisor-revoke",
                            expected_revision=receipts["b"]["revision"])["receipt"]
        require(stop["generation"] > receipts["b"]["generation"], "durable revoke reused generation")
        failure = live.event(revoked["run_id"])
        require(failure["event"] == "failed" and failure["code"] == "revoked",
                "revoked pending run was not closed")
        start("b", ok=False)
        completed(start("a"))
        checks.append("durable revocation cancels B and does not change A")

        for _ in range(3):
            start("launch", "launch_failure", ok=False)
            completed(start("a"))
        status = live.command("status")["status"]
        require(manifests["launch"] in status["needs_repair"], "repeated failures were not bounded")
        launch = next(row for row in status["installations"] if row["package_id"] == manifests["launch"])
        require(launch["generation"] > receipts["launch"]["generation"],
                "launch failure threshold did not persist disabled authority")
        start("launch", ok=False)
        checks.append("three launch failures persist disabled authority; no unbounded restart loop")
        for _ in range(3):
            bad = start("bootstrap")
            failure = live.event(bad["run_id"])
            require(failure["event"] == "failed" and failure["code"] == "invalid_entrypoint",
                    "invalid entrypoint was not recognized as a real startup failure")
            completed(start("a"))
        status = live.command("status")["status"]
        require(manifests["bootstrap"] in status["needs_repair"],
                "real Wasm startup failures bypassed the bounded retry policy")
        start("bootstrap", ok=False)
        checks.append("three actual Wasm entrypoint failures require repair without stopping A")
        pending_shutdown = start("a", "block_host_call")
        status = pending_broker(pending_shutdown)
        require(any(row["run_id"] == pending_shutdown["run_id"] for row in status["active"]),
                "shutdown test did not begin with a live pending worker")
        report["persistent_supervisor"]["shutdown_pending_run"] = pending_shutdown["run_id"]
        checks.append("normal shutdown cancels a live pending worker and confirms reaping")
        require(live.process.pid == supervisor_pid and live.process.poll() is None,
                "tests switched or restarted the supervisor")

    for pid, identity in worker_start_times.items():
        require(process_start(pid) != identity, "normal shutdown left the same worker process unreaped")
    report["persistent_supervisor"]["worker_start_times"] = worker_start_times
    report["persistent_supervisor"]["normal_shutdown_workers_reaped"] = True
    action("B revoke survives supervisor exit", "resume", store, manifests["b"], *scope, ok=False)
    action("launch failure disabling survives supervisor exit", "resume", store,
           manifests["launch"], *scope, ok=False)
    action("real startup disabling survives supervisor exit", "resume", store,
           manifests["bootstrap"], *scope, ok=False)
    action("A authority remains valid after supervisor exit", "execute", store, manifests["a"], *scope)
    report["persistent_supervisor"].update(passed=True, checks=checks,
                                           same_supervisor_for_all_checks=True)
