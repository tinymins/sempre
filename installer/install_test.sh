#!/bin/sh

set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
installer="$root/installer/install.sh"
digest=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef

run_case() {
  platform=$1
  architecture=$2
  hash=$3
  want_success=$4
  directory=$(mktemp -d "${TMPDIR:-/tmp}/sempre-installer-test.XXXXXX")
  commands="$directory/commands"
  record="$directory/record"
  mkdir -p "$commands"

  cat >"$commands/uname" <<EOF
#!/bin/sh
case "\$1" in
-s) printf '%s\n' '$platform' ;;
-m) printf '%s\n' '$architecture' ;;
esac
EOF
  cat >"$commands/curl" <<EOF
#!/bin/sh
output=''
url=''
while [ \$# -gt 0 ]; do
  case "\$1" in
    -o) output=\$2; shift 2 ;;
    -w) shift 2 ;;
    -*) shift ;;
    *) url=\$1; shift ;;
  esac
done
case "\$url" in
  */releases/latest) printf '%s' 'https://github.com/tinymins/sempre/releases/tag/v0.1.0' ;;
  */SHA256SUMS) printf '%s  %s\n' '$digest' 'sempre-bundle-__PLATFORM__-__ARCH__.zip' >"\$output" ;;
  *) printf 'archive' >"\$output" ;;
esac
EOF
  sed -i "s/__PLATFORM__/$([ "$platform" = Darwin ] && printf darwin || printf linux)/g; s/__ARCH__/$([ "$architecture" = arm64 ] && printf arm64 || printf amd64)/g" "$commands/curl"
  cat >"$commands/sha256sum" <<EOF
#!/bin/sh
printf '%s  %s\n' '$hash' "\$1"
EOF
  cat >"$commands/shasum" <<EOF
#!/bin/sh
shift 2
printf '%s  %s\n' '$hash' "\$1"
EOF
  cat >"$commands/unzip" <<'EOF'
#!/bin/sh
destination=''
while [ $# -gt 0 ]; do
  case "$1" in
    -d) destination=$2; shift 2 ;;
    *) shift ;;
  esac
done
name=$(basename "$(dirname "$0")")
case "$SEMPRE_TEST_PLATFORM/$SEMPRE_TEST_ARCH" in
  Linux/x86_64) prefix=sempre-linux-amd64 ;;
  Darwin/arm64) prefix=sempre-darwin-arm64 ;;
esac
mkdir -p "$destination/$prefix"
cat >"$destination/$prefix/sempre" <<'BINARY'
#!/bin/sh
printf '%s\n' "$*" >"$SEMPRE_TEST_RECORD"
BINARY
chmod 755 "$destination/$prefix/sempre"
EOF
  chmod 755 "$commands"/*

  if PATH="$commands:$PATH" SEMPRE_TEST_PLATFORM="$platform" SEMPRE_TEST_ARCH="$architecture" SEMPRE_TEST_RECORD="$record" sh "$installer" >/dev/null 2>"$directory/error"; then
    result=success
  else
    result=failure
  fi
  if [ "$result" != "$want_success" ]; then
    cat "$directory/error" >&2
    rm -rf "$directory"
    printf 'case %s/%s: got %s, want %s\n' "$platform" "$architecture" "$result" "$want_success" >&2
    exit 1
  fi
  if [ "$want_success" = success ]; then
    [ "$(cat "$record")" = install ] || exit 1
  else
    [ ! -e "$record" ] || exit 1
  fi
  rm -rf "$directory"
}

run_case Linux x86_64 "$digest" success
run_case Darwin arm64 "$digest" success
run_case Linux x86_64 ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff failure
