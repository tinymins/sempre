#!/bin/sh

set -eu

repository="https://github.com/tinymins/sempre"
temporary_directory=""

fail() {
  printf 'sempre installer: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

cleanup() {
  if [ -n "$temporary_directory" ]; then
    rm -rf -- "$temporary_directory"
  fi
}

trap cleanup EXIT
trap 'exit 1' HUP INT TERM

require_command curl
require_command unzip

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

latest_url="$repository/releases/latest"
effective_url="$(curl -fsSL -o /dev/null -w '%{url_effective}' "$latest_url")"
case "$effective_url" in
"$repository/releases/tag/"*) tag=${effective_url#"$repository/releases/tag/"} ;;
*) fail "could not resolve the latest release tag" ;;
esac
tag=${tag%/}
case "$tag" in
v[0-9]*) ;;
*) fail "invalid release tag: $tag" ;;
esac
case "$tag" in
*[!0-9A-Za-z._-]*) fail "invalid release tag: $tag" ;;
esac

asset="sempre-bundle-$platform-$architecture.zip"
release_base="$repository/releases/download/$tag"
temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/sempre-install.XXXXXX")"
archive="$temporary_directory/$asset"
checksums="$temporary_directory/SHA256SUMS"

printf 'Downloading Sempre %s for %s/%s...\n' "$tag" "$platform" "$architecture"
curl -fsSL "$release_base/SHA256SUMS" -o "$checksums"
curl -fsSL "$release_base/$asset" -o "$archive"

expected="$(awk -v asset="$asset" '$2 == asset || $2 == "*" asset { print $1 }' "$checksums")"
case "$expected" in
'' | *[!0-9A-Fa-f]*) fail "checksum for $asset is missing or invalid" ;;
esac
if [ "${#expected}" -ne 64 ]; then
  fail "checksum for $asset is missing or invalid"
fi

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
"$binary" install
printf 'Sempre %s installed successfully. Open a new terminal and run: sempre status\n' "$tag"
