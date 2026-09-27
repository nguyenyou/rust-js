#!/usr/bin/env bash
# Build serde, serde_derive and serde_json with rust-js's pinned toolchain
# (ADR 0077), and print the rustc flags that find them:
#
#   rust-js shared.rs -- $(serde/build.sh)           # metadata, for rust-js
#   rustc shared.rs $(serde/build.sh --rlib)         # libraries, for a native build
set -euo pipefail
cd "$(dirname "$0")"
kind=rmeta
[ "${1:-}" = "--rlib" ] && kind=rlib
target=../target/serde
cargo build --quiet --locked --target-dir "$target"
deps=$(cd "$target/debug/deps" && pwd)
latest() { ls -t "$deps"/lib"$1"-*."$kind" | head -1; }
echo "--extern serde=$(latest serde) --extern serde_json=$(latest serde_json) -L dependency=$deps"
