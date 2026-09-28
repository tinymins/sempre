#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

dev_dir=.cache/sempre-toolbox-dev
password_file="$dev_dir/postgres-password"
mkdir -p "$dev_dir"
chmod 700 "$dev_dir"
if [[ ! -f "$password_file" ]]; then
  umask 077
  openssl rand -hex 24 > "$password_file"
fi

export POSTGRES_PASSWORD
POSTGRES_PASSWORD="$(cat "$password_file")"
docker compose -p sempre-toolbox-dev -f scripts/server-dev.compose.yml up -d --wait postgres

export DATABASE_URL="postgres://sempre:${POSTGRES_PASSWORD}@127.0.0.1:15433/toolbox"
export SEMPRE_BIND=127.0.0.1:8788
export SEMPRE_PUBLIC_URL=http://127.0.0.1:5174/
export SEMPRE_WEB_ROOT=../server-ui/dist
export SEMPRE_SERVER_API_TARGET=http://127.0.0.1:8788

proxy_env_file="$dev_dir/direct-proxy.env"
if [[ -f "$proxy_env_file" ]]; then
  # shellcheck disable=SC1090
  source "$proxy_env_file"
fi

echo "Sempre server development: API http://127.0.0.1:8788, UI http://127.0.0.1:5174"
case "${1:-}" in
  '') exec bun run --parallel "server:dev:api" "server:dev:ui" ;;
  --api-only) exec bun run "server:dev:api" ;;
  --migrate) exec cargo run --manifest-path=rust/Cargo.toml -p sempre-server -- migrate ;;
  --migrate-built)
    if [[ ! -x rust/target/debug/sempre-server ]]; then
      echo "Build sempre-server with Cargo Watch before using --migrate-built" >&2
      exit 1
    fi
    exec rust/target/debug/sempre-server migrate
    ;;
  *) echo "Usage: $0 [--api-only|--migrate|--migrate-built]" >&2; exit 2 ;;
esac
