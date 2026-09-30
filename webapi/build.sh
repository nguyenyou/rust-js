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
../builtins/build.sh -o "$dir/libjs.rmeta" "$@"
target=(--target=wasm32-unknown-unknown)
for arg in "$@"; do
  case "$arg" in --target | --target=*) target=() ;; esac
done
# rust-js compiles it, as rustc with rust-js's tool, `rust_js`, known (ADR
# 0112): $RUST_JS_COMPILER, or this repository's own build.
compiler=${RUST_JS_COMPILER:-$PWD/../target/debug/rust-js}
# A packaged one is a JS launcher, run by the JS runtime the build is.
rust_js=("$compiler")
case "$compiler" in *.js) rust_js=("${RUST_JS_JS_RUNTIME:-node}" "$compiler") ;; esac
exec "${rust_js[@]}" --rustc --edition=2024 --crate-type=lib --crate-name=webapi --emit=metadata ${target[@]+"${target[@]}"} src/lib.rs \
  --extern js="$dir/libjs.rmeta" -o "$dir/$(basename "$out")" "$@"
