#!/bin/sh

set -eu

manifest_url="https://sempre.run/api/releases/latest.env"
temporary_directory=""
core=""
subscription=""
ui=""
ui_sha256=""

fail() {
  printf 'sempre installer: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

manifest_value() {
  key=$1
  value=$(awk -v prefix="$key=" '
    index($0, prefix) == 1 { count += 1; result = substr($0, length(prefix) + 1) }
    END { if (count == 1 && result != "") print result; else exit 1 }
  ' "$manifest") || fail "release manifest has an invalid $key field"
  printf '%s' "$value"
}

cleanup() {
  if [ -n "$temporary_directory" ]; then
    rm -rf -- "$temporary_directory"
  fi
}

trap cleanup EXIT
trap 'exit 1' HUP INT TERM

while [ "$#" -gt 0 ]; do
  case "$1" in
  --core=* | --subscription=* | --ui=* | --ui-sha256=*)
    option=${1%%=*}
    value=${1#*=}
    shift
    ;;
  --core | --subscription | --ui | --ui-sha256)
    option=$1
    shift
    [ "$#" -gt 0 ] || fail "$option requires a value"
    [ "${1#--}" = "$1" ] || fail "$option requires a value"
    value=$1
    shift
    ;;
  *) fail "unknown option: $1" ;;
  esac
  [ -n "$value" ] || fail "$option cannot be empty"
  case "$option" in
  --core)
    [ -z "$core" ] || fail "$option was provided more than once"
    core=$value
    ;;
  --subscription)
    [ -z "$subscription" ] || fail "$option was provided more than once"
    subscription=$value
    ;;
  --ui)
    [ -z "$ui" ] || fail "$option was provided more than once"
    ui=$value
    ;;
  --ui-sha256)
    [ -z "$ui_sha256" ] || fail "$option was provided more than once"
    ui_sha256=$value
    ;;
  esac
done

require_command curl
require_command unzip
require_command awk

case "$(uname -s)" in
Linux)
  platform="linux"
  require_command sha256sum
  ;;
Darwin)
  platform="darwin"
  require_command shasum
  ;;
*)
  fail "unsupported operating system: $(uname -s)"
  ;;
esac

case "$(uname -m)" in
x86_64 | amd64)
  architecture="amd64"
  ;;
arm64 | aarch64)
  architecture="arm64"
  ;;
*)
  fail "unsupported architecture: $(uname -m)"
  ;;
esac

asset="sempre-bundle-$platform-$architecture.zip"
temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/sempre-install.XXXXXX")"
manifest="$temporary_directory/latest.env"
archive="$temporary_directory/$asset"
curl -fsSL "$manifest_url" -o "$manifest"

[ "$(manifest_value schema)" = 1 ] || fail "unsupported release manifest schema"
version=$(manifest_value version)
repository=$(manifest_value repository)
target="${platform}_${architecture}"
asset_url=$(manifest_value "asset_${target}_url")
expected=$(manifest_value "asset_${target}_sha256")
case "$version" in
[0-9]*.[0-9]*.[0-9]*) ;;
*) fail "invalid release version: $version" ;;
esac
case "$version" in
*[!0-9A-Za-z.+-]*) fail "invalid release version: $version" ;;
esac
printf '%s\n' "$version" | awk '/^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$/ { valid = 1 } END { exit !valid }' || fail "invalid release version: $version"
case "$repository" in
https://*) ;;
*) fail "invalid release repository URL" ;;
esac
case "$asset_url" in
https://*) ;;
*) fail "invalid release asset URL" ;;
esac
case "$expected" in
'' | *[!0-9A-Fa-f]*) fail "checksum for $asset is missing or invalid" ;;
esac
[ "${#expected}" -eq 64 ] || fail "checksum for $asset is missing or invalid"

printf 'Downloading Sempre %s for %s/%s...\n' "$version" "$platform" "$architecture"
curl -fsSL "$asset_url" -o "$archive"

if [ "$platform" = "darwin" ]; then
  actual="$(shasum -a 256 "$archive" | awk '{ print $1 }')"
else
  actual="$(sha256sum "$archive" | awk '{ print $1 }')"
fi
if [ "$actual" != "$expected" ]; then
  fail "SHA-256 verification failed for $asset"
fi

bundle_directory="$temporary_directory/bundle"
mkdir -p "$bundle_directory"
unzip -q "$archive" -d "$bundle_directory"
binary="$bundle_directory/sempre-$platform-$architecture/sempre"
if [ ! -f "$binary" ]; then
  fail "verified bundle does not contain the Sempre executable"
fi
chmod 755 "$binary"

printf 'Installing Sempre system service...\n'
set -- install
if [ -n "$core" ]; then
  set -- "$@" "--core=$core"
fi
if [ -n "$subscription" ]; then
  subscription_file="$temporary_directory/subscription-url"
  (umask 077 && printf '%s' "$subscription" >"$subscription_file")
  set -- "$@" "--subscription-file=$subscription_file"
fi
if [ -n "$ui" ]; then
  set -- "$@" "--ui=$ui"
fi
if [ -n "$ui_sha256" ]; then
  set -- "$@" "--ui-sha256=$ui_sha256"
fi
"$binary" "$@"
printf 'Sempre %s installed successfully. Open a new terminal and run: sempre status\n' "$version"
