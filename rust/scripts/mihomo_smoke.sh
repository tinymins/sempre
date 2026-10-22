#!/usr/bin/env bash
# Logs and status files intentionally belong to the unprivileged CI runner.
# shellcheck disable=SC2024
set -euo pipefail

: "${MIHOMO_VERSION:?Set the Mihomo version to validate}"
test -x smoke/sempre
cat > smoke/subscription.yaml <<'EOF'
proxies:
  - name: smoke
    type: ss
    server: 127.0.0.1
    port: 443
    cipher: aes-128-gcm
    password: smoke-test
EOF
sudo ./smoke/sempre --portable core install "mihomo@${MIHOMO_VERSION}"
sudo ./smoke/sempre --portable core use "mihomo@${MIHOMO_VERSION}"
sudo ./smoke/sempre --portable config import smoke/subscription.yaml

sudo ./smoke/sempre --portable daemon > smoke/daemon.log 2>&1 &
sempre_pid=$!
cleanup() {
  sudo kill -TERM "$sempre_pid" 2>/dev/null || true
  wait "$sempre_pid" 2>/dev/null || true
  cat smoke/daemon.log
}
trap cleanup EXIT

# The core's HTTP API can answer during the supervisor's startup grace period.
# Wait for the daemon's Running contract as well as the core API, not just HTTP.
ready=false
deadline=$((SECONDS + 30))
while ((SECONDS < deadline)); do
  if ! kill -0 "$sempre_pid" 2>/dev/null; then
    echo 'Sempre daemon exited before Mihomo became ready' >&2
    exit 1
  fi
  if sudo test -f smoke/.sempre/run/sempre-control.json &&
    sudo ./smoke/sempre --portable --json runtime status > smoke/status.json; then
    if jq -e '.runtime_state == "failed"' smoke/status.json > /dev/null; then
      cat smoke/status.json
      exit 1
    fi
    if jq -e '.runtime_state == "running" and .pid > 0' smoke/status.json > /dev/null &&
      sudo test -f smoke/.sempre/run/control.json; then
      control=$(sudo cat smoke/.sempre/run/control.json)
      base_url=$(jq -r '.base_url // empty' <<<"$control")
      secret=$(jq -r '.secret // empty' <<<"$control")
      core=$(jq -r '.core // empty' <<<"$control")
      if [[ "$core" == 'mihomo' && -n "$base_url" && -n "$secret" ]] &&
        curl --fail --silent --max-time 2 -H "Authorization: Bearer $secret" "$base_url/version" > smoke/version.json; then
        ready=true
        break
      fi
    fi
  fi
  sleep 0.2
done

if [[ "$ready" != true ]]; then
  echo 'Mihomo did not reach Running with a responsive control API within 30 seconds' >&2
  if [[ -f smoke/status.json ]]; then cat smoke/status.json; fi
  exit 1
fi
cat smoke/status.json
jq -e --arg version "$MIHOMO_VERSION" '.version | contains($version)' smoke/version.json
curl --fail --silent --max-time 10 -H "Authorization: Bearer $secret" "$base_url/proxies" | jq -e '.proxies | type == "object"'
curl --fail --silent --max-time 10 -H "Authorization: Bearer $secret" "$base_url/connections" | jq -e '.connections | type == "array"'
curl --fail --silent --max-time 10 -H "Authorization: Bearer $secret" "$base_url/rules" | jq -e '.rules | type == "array"'
