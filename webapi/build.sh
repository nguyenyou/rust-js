#!/usr/bin/env bash
# Compile the webapi crate's metadata, which is all programs need (ADR 0024),
# and the js crate's beside it, which it uses (ADR 0102), for the target
# rust-js checks programs for (ADR 0090):
#
#   webapi/build.sh -o target/libwebapi.rmeta
#
# Then compile a program with
# `rust-js app.rs -- --extern webapi=<that file> --extern js=<dir>/libjs.rmeta -L <dir>`.
set -euo pipefail
[ "${1:-}" = "-o" ] && [ -n "${2:-}" ] || {
  echo "usage: webapi/build.sh -o <dir>/libwebapi.rmeta [rustc flags]" >&2
  exit 1
}
out=$2
shift 2
mkdir -p "$(dirname "$out")"
dir=$(cd "$(dirname "$out")" && pwd)
cd "$(dirname "$0")"
../js/build.sh -o "$dir/libjs.rmeta" "$@"
target=(--target=wasm32-unknown-unknown)
for arg in "$@"; do
  case "$arg" in --target | --target=*) target=() ;; esac
done
exec rustc --edition=2024 --crate-type=lib --crate-name=webapi --emit=metadata ${target[@]+"${target[@]}"} src/lib.rs \
  --extern js="$dir/libjs.rmeta" -o "$dir/$(basename "$out")" "$@"
