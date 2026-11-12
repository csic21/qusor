#!/usr/bin/env python3
"""Build the ordinary signed packages the app ships and stage them beside the native crate.

The packages are built from `plugins/` outside the app workspace, exactly as the
extension records require; the app only embeds the resulting `.qpk` and the
publisher key they were signed with. Run this before `cargo check`/`nsh package`
on a clean checkout.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

APP = Path(__file__).resolve().parent
ROOT = APP.parents[1]
ASSETS = APP / "native" / "assets"


def main() -> int:
    ASSETS.mkdir(parents=True, exist_ok=True)
    for name in ("notes", "vocab", "reader"):
        builder = ROOT / "plugins" / name / "build.py"
        subprocess.run([sys.executable, str(builder)], cwd=ROOT, check=True)
        target = ROOT / "plugins" / name / "target"
        proof = json.loads((target / "author-proof.json").read_text(encoding="utf-8"))
        shutil.copyfile(target / f"{name}.qpk", ASSETS / f"{name}.qpk")
        (ASSETS / f"{name}.pub").write_text(proof["publisher_key"].strip() + "\n", encoding="utf-8")
        print(f"{name}: {proof['package']['sha256']} -> {ASSETS / (name + '.qpk')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
