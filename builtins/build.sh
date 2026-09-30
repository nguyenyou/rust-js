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
# The crate uses rustc's unstable features, which a stable release lets only
# the crates `RUSTC_BOOTSTRAP` names use (ADR 0109).
RUSTC_BOOTSTRAP=js exec rustc --edition=2024 --crate-type=lib --crate-name=js --emit=metadata ${target[@]+"${target[@]}"} src/lib.rs "$@"
