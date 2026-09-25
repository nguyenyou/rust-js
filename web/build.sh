#!/usr/bin/env bash
# Compile the web crate's metadata, which is all programs need (ADR 0024):
#
#   web/build.sh -o target/libweb.rmeta                                  # for the host
#   web/build.sh --target wasm32-unknown-unknown -o <dir>/libweb.rmeta   # for the playground
#
# Then compile a program with `rust-js app.rs -- --extern web=<that file>`.
set -euo pipefail
cd "$(dirname "$0")"
exec rustc --edition=2024 --crate-type=lib --crate-name=web --emit=metadata src/lib.rs "$@"
