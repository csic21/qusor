"""Cross-process signed private-state migration/rollback validation.

Imported by validate.py; all data and signing keys are public synthetic fixtures.
"""
from __future__ import annotations

import json
import sqlite3
import time


def validate_private(builder, fixtures, outside, runner, key, report, command, host_action, digest):
    def require(condition, message):
        if not condition:
            raise RuntimeError(message)

    def action(name, *args, **kwargs):
        return host_action(runner, "private: " + name, args, **kwargs)

    package_id = "org.example.private-tools"
    scope = ("synthetic-library", "synthetic-document", "version-1")
    expiry = int(time.time()) + 3600
    initial_state = b"theme=night"
    migrated_state = initial_state + b"\nlayout=wide"
    state_file = outside / "private-state.txt"
    state_file.write_bytes(initial_state)
    base_manifest = json.loads((fixtures / "manifest.json").read_text(encoding="utf-8"))
    base_manifest.update(package_id=package_id, required_capabilities=["document.metadata.read"],
                         optional_capabilities=[], contributions=[])
    packages = {}
    cases = {
        "v1": (1, "private-run-v1", None, 1, [], 1),
        "v2": (2, "private-run-v2", "private-migrate-good", 2, [1], 1),
        "trap": (2, "private-run-v2", "private-migrate-trap", 2, [1], 1),
        "loop": (2, "private-run-v2", "private-migrate-loop", 2, [1], 1),
        "bad-pointer": (2, "private-run-v2", "private-migrate-bad-pointer", 2, [1], 1),
        "oversize": (2, "private-run-v2", "private-migrate-oversize", 2, [1], 1),
        "import": (2, "private-run-v2", "private-migrate-import", 2, [1], 1),
        "wrong-schema": (2, "private-run-v2", "private-migrate-wrong-schema", 2, [1], 1),
        "bad-abi": (2, "private-run-bad-abi", "private-migrate-good", 2, [1], 1),
        "host-call": (2, "private-run-host-call", "private-migrate-good", 2, [1], 1),
        "unsupported-source": (2, "private-run-v2", "private-migrate-good", 2, [99], 1),
        "unknown-protocol": (2, "private-run-v2", "private-migrate-good", 2, [1], 99),
    }
    for name, (version, module, migration, schema, accepted, protocol) in cases.items():
        manifest = dict(base_manifest, package_version={"major": version, "minor": 0, "patch": 0})
        manifest_path = outside / f"private-{name}-manifest.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        lifecycle = {"protocol": protocol, "private_schema": schema,
                     "migration_module": "migration.wasm" if migration else None,
                     "accepted_from_schemas": accepted}
        descriptor = outside / f"private-{name}-lifecycle.json"
        descriptor.write_text(json.dumps(lifecycle), encoding="utf-8")
        packages[name] = outside / f"private-{name}.qpkg"
        command([builder, fixtures / (module + ".wat"), manifest_path, fixtures / "asset.txt",
                 fixtures / "test-seed.hex", packages[name], descriptor,
                 fixtures / (migration + ".wat") if migration else "-"], cwd=outside)
    report["private_package_sha256"] = {name: digest(path) for name, path in packages.items()}
    scratch = outside / "private-stores"
    scratch.mkdir(mode=0o700)

    def status(name, store):
        return action(name, "private-status", store, package_id)["private"]

    def check_head(name, store, version, state):
        observed = status(name, store)
        head = observed["current"]
        if version is None:
            require(head is None, name + ": unexpected initialized private head")
        else:
            require(head is not None and head["schema"] == version, name + ": wrong private schema")
            require(bytes.fromhex(head["state_hex"]) == state, name + ": wrong private bytes")
            require(head["package_digest"] == observed["installation"]["digest"],
                    name + ": package and state are mismatched")
        return observed

    def prepare(name, *, grant=False):
        store = scratch / name
        installed = action(name + " install", "install", store, "install", 0, packages["v1"], key)["receipt"]
        initialized = action(name + " initialize", "initialize-private", store, "initialize",
                             installed["revision"], package_id, state_file)["receipt"]
        require(initialized["outcome"] == "succeeded", "initialization failed")
        current = initialized["installation"]
        if grant:
            current = action(name + " grant", "grant", store, "grant", current["revision"],
                             package_id, *scope, expiry)["receipt"]
        check_head(name + " initial pair", store, 1, initial_state)
        return store, current

    store, old = prepare("normal", grant=True)
    action("ordinary install cannot bypass migration", "install", store, "bypass", old["revision"],
           packages["v2"], key, ok=False)
    upgraded = action("upgrade", "upgrade-private", store, "upgrade", old["revision"],
                      packages["v2"], key)["receipt"]
    require(upgraded["outcome"] == "succeeded", "upgrade did not succeed")
    state = check_head("upgraded pair", store, 2, migrated_state)
    require(state["installation"]["generation"] > old["generation"], "upgrade reused generation")
    action("upgraded pair does not inherit grant", "resume", store, package_id, *scope, ok=False)
    replay = action("upgrade replay", "upgrade-private", store, "upgrade", old["revision"],
                    packages["v2"], key)["receipt"]
    require(replay == upgraded, "upgrade replay changed terminal receipt")
    action("private operation id conflicts with ordinary command", "disable", store, "upgrade",
           state["installation"]["revision"], package_id, ok=False)
    previous = [pair for pair in state["retained"] if pair["schema"] == 1]
    require(bool(previous), "upgrade did not retain previous pair")
    granted = action("grant upgraded pair", "grant", store, "grant-v2", state["installation"]["revision"],
                     package_id, *scope, expiry)["receipt"]
    rolled = action("rollback", "rollback-private", store, "rollback", granted["revision"],
                    package_id, previous[0]["pair_id"])["receipt"]
    require(rolled["outcome"] == "succeeded", "rollback failed")
    state = check_head("rolled back exact old bytes", store, 1, initial_state)
    require(state["installation"]["generation"] > granted["generation"], "rollback reused generation")
    action("rollback does not revive grants", "resume", store, package_id, *scope, ok=False)

    failures = {"trap": "private_vm_trap", "loop": "private_fuel_exhausted",
                "bad-pointer": "private_invalid_pointer", "oversize": "private_state_size_limit",
                "import": "private_import_denied", "wrong-schema": "private_vm_trap",
                "bad-abi": "private_invalid_abi", "host-call": "private_capability_denied"}
    for name, expected_error in failures.items():
        store, old = prepare(name, grant=True)
        args = ("upgrade-private", store, "upgrade", old["revision"], packages[name], key)
        failed = action(name + " migration rejected", *args)["receipt"]
        require(failed["outcome"] == "failed" and failed["reason"] == expected_error,
                name + ": failed at an unrelated boundary: " + str(failed))
        observed = check_head(name + " old pair retained", store, 1, initial_state)
        require(observed["installation"]["generation"] > old["generation"], name + ": old generation survived")
        action(name + " stopped authorization", "resume", store, package_id, *scope, ok=False)
        repeated = action(name + " failed receipt replay", *args)["receipt"]
        require(repeated == failed, name + ": failure replay reran migration")

    for name in ("unsupported-source", "unknown-protocol"):
        store, old = prepare(name, grant=True)
        action(name + " validation rejects before stop", "upgrade-private", store, "upgrade",
               old["revision"], packages[name], key, ok=False)
        observed = check_head(name + " retains original pair", store, 1, initial_state)
        require(observed["installation"] == old | {"operation_id": ""}, name + ": precheck mutated installation")
        action(name + " prior grant remains usable", "resume", store, package_id, *scope)

    points = ("before_prepare", "after_prepare", "after_migration", "after_snapshot", "before_commit", "after_commit")
    for point in points:
        store, old = prepare("crash-" + point, grant=True)
        args = ("upgrade-private", store, "upgrade", old["revision"], packages["v2"], key)
        action("upgrade crash " + point, *args, "--crash", point, crash=True)
        committed = point == "after_commit"
        observed = check_head("upgrade recover " + point, store, 2 if committed else 1,
                              migrated_state if committed else initial_state)
        action("upgrade recover authority " + point, "resume", store, package_id, *scope,
               ok=point == "before_prepare")
        retried = action("upgrade retry " + point, *args)["receipt"]
        expected = "succeeded" if point in ("before_prepare", "after_commit") else "interrupted"
        require(retried["outcome"] == expected, point + ": wrong retry outcome")
        again = action("upgrade second retry " + point, *args)["receipt"]
        require(again == retried, point + ": retry duplicated terminal state")
        if point != "before_prepare":
            require(observed["installation"]["generation"] > old["generation"], point + ": stale generation recovered")

    for point in points:
        store, old = prepare("rollback-crash-" + point)
        upgraded = action("rollback crash base " + point, "upgrade-private", store, "upgrade",
                          old["revision"], packages["v2"], key)["receipt"]
        observed = check_head("rollback prepare pair " + point, store, 2, migrated_state)
        pair = next(pair for pair in observed["retained"] if pair["schema"] == 1)
        args = ("rollback-private", store, "rollback", upgraded["installation"]["revision"], package_id, pair["pair_id"])
        action("rollback crash " + point, *args, "--crash", point, crash=True)
        committed = point == "after_commit"
        check_head("rollback recover " + point, store, 1 if committed else 2,
                   initial_state if committed else migrated_state)
        retried = action("rollback retry " + point, *args)["receipt"]
        require(retried["outcome"] == ("succeeded" if point in ("before_prepare", "after_commit") else "interrupted"),
                point + ": rollback resumed from incorrect phase")

    for point in ("before_prepare", "after_snapshot", "before_commit", "after_commit"):
        store = scratch / ("init-crash-" + point)
        old = action("init prepare " + point, "install", store, "install", 0, packages["v1"], key)["receipt"]
        args = ("initialize-private", store, "initialize", old["revision"], package_id, state_file)
        action("init crash " + point, *args, "--crash", point, crash=True)
        check_head("init recover " + point, store, 1 if point == "after_commit" else None,
                   initial_state if point == "after_commit" else None)
        retried = action("init retry " + point, *args)["receipt"]
        require(retried["outcome"] == "succeeded", "init retry did not complete")

    store, old = prepare("reinstall")
    action("uninstall private owner", "uninstall", store, "remove", old["revision"], package_id)
    reinstalled = action("reinstall has fresh identity", "install", store, "reinstall", 0,
                         packages["v1"], key)["receipt"]
    require(reinstalled["installation_id"] != old["installation_id"], "reinstall reused private owner")
    check_head("reinstall has no private head", store, None, None)
    action("old initialize receipt cannot initialize new instance", "initialize-private", store,
           "initialize", 1, package_id, state_file)
    check_head("old receipt does not resurrect private head", store, None, None)

    for mutation in ("orphan-owner", "bad-input-hash", "wrong-target", "stale-generation"):
        store, old = prepare("corrupt-pending-" + mutation)
        action("prepare pending corruption " + mutation, "upgrade-private", store, "upgrade",
               old["revision"], packages["v2"], key, "--crash", "after_prepare", crash=True)
        with sqlite3.connect(store / "state.sqlite3") as db:
            owner, text = db.execute("SELECT installation_id,record FROM private_pending").fetchone()
            record = json.loads(text)
            if mutation == "orphan-owner":
                db.execute("UPDATE private_pending SET installation_id=999999")
            else:
                if mutation == "bad-input-hash":
                    record["input"] = "z" * 64
                elif mutation == "wrong-target":
                    record["target_digest"] = "0" * 64
                else:
                    record["installation"]["generation"] -= 1
                db.execute("UPDATE private_pending SET record=? WHERE installation_id=?",
                           (json.dumps(record), owner))
        action("corrupt pending fails closed " + mutation, "status", store, ok=False)
        with sqlite3.connect(store / "state.sqlite3") as db:
            require(db.execute("SELECT count(*) FROM private_pending").fetchone()[0] == 1,
                    "corrupt pending was silently converted to a recovery receipt")
