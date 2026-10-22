#!/usr/bin/env bash
# Install, drive, and uninstall the native Rust service on Linux or macOS.
set -euo pipefail

cargo build --manifest-path=rust/Cargo.toml \
  -p sempre-client -p sempre-testcore -p sempre-smoke-setup
mkdir smoke
cp rust/target/debug/sempre smoke/sempre
cargo run --manifest-path=rust/Cargo.toml -p sempre-smoke-setup -- \
  --root "$PWD/smoke" --core "$PWD/rust/target/debug/sempre-testcore"
sudo ./smoke/sempre --portable service install --yes
./smoke/sempre service status | grep -q running
sudo ./smoke/sempre --portable service install --yes
./smoke/sempre service status | grep -q running
wait_runtime() {
  desired="$1" state="$2" runtime_status=''
  for _ in $(seq 1 100); do
    runtime_status=$(sudo ./smoke/sempre --system runtime status 2>/dev/null || true)
    if printf '%s\n' "$runtime_status" | grep -q "Desired: $desired" && \
      printf '%s\n' "$runtime_status" | grep -q "State: $state"; then
      return 0
    fi
    sleep 0.2
  done
  printf 'managed core did not reach %s/%s:\n%s\n' \
    "$desired" "$state" "$runtime_status" >&2
  return 1
}
wait_runtime running running
sudo ./smoke/sempre --portable service deploy core | grep -q 'core deployment completed'
wait_runtime running running
sudo ./smoke/sempre --portable service deploy data | grep -q 'data deployment completed'
wait_runtime running running
sudo ./smoke/sempre --system runtime stop >/dev/null
wait_runtime stopped stopped
sudo ./smoke/sempre --portable service restart
wait_runtime stopped stopped
printf '%s\n' 'trojan://secret@example.com:443#smoke-updated' > smoke/subscription-updated.txt
sudo ./smoke/sempre --system config import smoke/subscription-updated.txt | grep -q staged
wait_runtime stopped stopped
sudo ./smoke/sempre --system runtime start >/dev/null
wait_runtime running running
sudo ./smoke/sempre --system runtime restart >/dev/null
wait_runtime running running
sudo ./smoke/sempre --portable service stop
sudo ./smoke/sempre --portable service start
sudo ./smoke/sempre --portable service restart
sudo ./smoke/sempre --portable service uninstall
