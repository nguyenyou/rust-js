#!/usr/bin/env bash
# Stage the sysroot rust-js type-checks against (wasm/sysroot): the official
# wasm32-unknown-unknown metadata from the pinned toolchain. The front end
# only reads `.rmeta` files; `.rlib`s (object code) aren't needed.
set -euo pipefail
cd "$(dirname "$0")"

TOOLCHAIN=nightly-2026-03-25

rustup target add wasm32-unknown-unknown --toolchain "$TOOLCHAIN" >/dev/null
SYSROOT=$(rustc "+$TOOLCHAIN" --print sysroot)
LIB=sysroot/lib/rustlib/wasm32-unknown-unknown/lib
rm -rf sysroot && mkdir -p "$LIB"
cp "$SYSROOT/lib/rustlib/wasm32-unknown-unknown/lib/"*.rmeta "$LIB/"
echo "staged: $PWD/sysroot"
