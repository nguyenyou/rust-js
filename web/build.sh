#!/usr/bin/env bash
# Compile the web crate's metadata, which is all programs need (ADR 0024),
# for the target rust-js checks programs for (ADR 0090):
#
#   web/build.sh -o target/libweb.rmeta
#
# Then compile a program with `rust-js app.rs -- --extern web=<that file>`.
set -euo pipefail
cd "$(dirname "$0")"
target=(--target=wasm32-unknown-unknown)
for arg in "$@"; do
  case "$arg" in --target | --target=*) target=() ;; esac
done
exec rustc --edition=2024 --crate-type=lib --crate-name=web --emit=metadata ${target[@]+"${target[@]}"} src/lib.rs "$@"
