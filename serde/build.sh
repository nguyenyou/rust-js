#!/usr/bin/env bash
# Build serde, serde_derive and serde_json with rust-js's pinned toolchain
# (ADR 0077), and print the rustc flags that find them:
#
#   rust-js shared.rs -- $(serde/build.sh)           # metadata, for rust-js's target (ADR 0090)
#   rustc shared.rs $(serde/build.sh --rlib)         # libraries, for a native build
set -euo pipefail
cd "$(dirname "$0")"
target=../target/serde
if [ "${1:-}" = "--rlib" ]; then
  cargo build --quiet --locked --target-dir "$target"
  deps=$(cd "$target/debug/deps" && pwd)
  latest() { ls -t "$deps"/lib"$1"-*.rlib | head -1; }
  echo "--extern serde=$(latest serde) --extern serde_json=$(latest serde_json) -L dependency=$deps"
else
  # For the target; serde_derive, a procedural macro, runs on the host.
  cargo build --quiet --locked --target-dir "$target" --target wasm32-unknown-unknown
  deps=$(cd "$target/wasm32-unknown-unknown/debug/deps" && pwd)
  host=$(cd "$target/debug/deps" && pwd)
  latest() { ls -t "$deps"/lib"$1"-*.rmeta | head -1; }
  echo "--extern serde=$(latest serde) --extern serde_json=$(latest serde_json) -L dependency=$deps -L dependency=$host"
fi
