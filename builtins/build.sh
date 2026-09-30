#!/usr/bin/env bash
# Compile the js crate's metadata, which is all programs need (ADR 0102),
# for the target rust-js checks programs for (ADR 0090):
#
#   builtins/build.sh -o target/libjs.rmeta
#
# Then compile a program with `rust-js app.rs -- --extern js=<that file>`.
set -euo pipefail
cd "$(dirname "$0")"
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
exec "${rust_js[@]}" --rustc --edition=2024 --crate-type=lib --crate-name=js --emit=metadata ${target[@]+"${target[@]}"} src/lib.rs "$@"
