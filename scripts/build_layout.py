"""Single source for the pinned NSH checkout and the shared Cargo target.

Live entry points import this module instead of spelling ``target/nsh``.
"""
from __future__ import annotations

import os
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
NSH_SRC = REPO / "third_party" / "nsh"
NSH_SHA = "913fd76e1f8434ec6cdf248f4476c1ebe08af19f"
CARGO_TARGET = REPO / ".build" / "cargo-target"


def nsh_cli(profile: str = "debug") -> Path:
    """Host ``nsh`` binary inside the shared Cargo target."""
    name = "nsh.exe" if os.name == "nt" else "nsh"
    return CARGO_TARGET / profile / name
