#!/usr/bin/env python3
"""Allow the RSA advisory exemption only while RSA is unreachable."""

from pathlib import Path
import subprocess
import sys


manifest = Path(__file__).resolve().parents[1] / "Cargo.toml"
# Inspect the forward graph: inverse lookup exits unsuccessfully when the package
# is absent, which would hide resolution failures if its exit status were ignored.
result = subprocess.run(
    [
        "cargo", "tree", "--manifest-path", str(manifest), "--locked",
        "--workspace", "--target", "all", "--prefix", "none", "--format", "{p}",
    ],
    check=True,
    stdout=subprocess.PIPE,
    text=True,
)
if not result.stdout.strip():
    sys.exit("Cargo returned an empty dependency graph; cannot verify RSA is unreachable")
for line in result.stdout.splitlines():
    if line.startswith("rsa "):
        sys.exit(f"{line} is reachable; cannot ignore RUSTSEC-2023-0071")

print("RSA is absent from every target's resolved workspace dependency graph.")
