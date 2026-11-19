#!/usr/bin/env python3
"""Verify that packages covered by lockfile audit exemptions are unreachable."""

from pathlib import Path
import re
import subprocess
import sys


ADVISORIES = {
    "rsa": "RUSTSEC-2023-0071",
    "libsqlite3-sys": "RUSTSEC-2026-0459",
    "sqlx-sqlite": "RUSTSEC-2026-0459",
}
# Cargo's {p} is a package name and version, optionally followed by a source
# location and/or the (*) marker for a repeated package.
PACKAGE = re.compile(r"([\w-]+) v[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.+-]+)?(?: \(.+\))*")
manifest = Path(__file__).resolve().parents[1] / "Cargo.toml"
# An inverse lookup also fails for an absent package. Inspect the forward
# graph so that a resolution error can never authorize an exemption.
result = subprocess.run(
    [
        "cargo", "tree", "--color", "never", "--manifest-path", str(manifest), "--locked",
        "--workspace", "--all-features", "--target", "all", "--prefix", "none", "--format", "{p}",
    ],
    check=True,
    stdout=subprocess.PIPE,
    text=True,
)
if not result.stdout.strip():
    sys.exit("Cargo returned an empty dependency graph; cannot verify audit exemptions")
for line in result.stdout.splitlines():
    if not line.strip():
        continue
    package = PACKAGE.fullmatch(line)
    if package is None:
        sys.exit(f"Unrecognized cargo tree output: {line!r}; cannot verify audit exemptions")
    if advisory := ADVISORIES.get(package[1]):
        sys.exit(f"{line} is reachable; cannot ignore {advisory}")

print("Verified all audit exemptions against the all-target, all-features workspace graph.")
