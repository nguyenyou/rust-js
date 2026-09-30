#!/usr/bin/env bash
# Compile the react crate's metadata (ADRs 0041, 0043), and the webapi and js
# crates' beside it, which it uses:
#
#   react/build.sh -o target/libreact.rmeta                  # the latest React's API
#   react/build.sh -o target/react-18.2/libreact.rmeta --react 18.2.0
#
# With `--react`, what later React versions added is left out, so a program
# using it doesn't compile. Then compile a program with
# `rust-js app.rs -- --extern react=target/libreact.rmeta -L target`.
set -euo pipefail
[ "${1:-}" = "-o" ] && [ -n "${2:-}" ] || {
  echo "usage: react/build.sh -o <dir>/libreact.rmeta [--react <version>] [rustc flags]" >&2
  exit 1
}
out=$2
shift 2
version=
if [ "${1:-}" = "--react" ]; then
  version=${2:?--react needs a version, like 18.2.0}
  shift 2
fi
mkdir -p "$(dirname "$out")"
dir=$(cd "$(dirname "$out")" && pwd)
cd "$(dirname "$0")"
cfg=()
cfg_output=$("${RUST_JS_JS_RUNTIME:-node}" cfg.js ${version:+"$version"})
while IFS= read -r flag; do cfg+=("$flag"); done <<< "$cfg_output"
../webapi/build.sh -o "$dir/libwebapi.rmeta" "$@"
# For rust-js's target (ADR 0090), unless another is given.
target=(--target=wasm32-unknown-unknown)
for arg in "$@"; do
  case "$arg" in --target | --target=*) target=() ;; esac
done
# The crate uses rustc's unstable features, which a stable release lets only
# the crates `RUSTC_BOOTSTRAP` names use (ADR 0109).
# The absolute path lets a program's errors quote this crate's source: a gated
# item's `#[cfg(react = "..")]`.
RUSTC_BOOTSTRAP=react exec rustc --edition=2024 --crate-type=lib --crate-name=react --emit=metadata ${target[@]+"${target[@]}"} "$PWD/src/lib.rs" \
  --extern webapi="$dir/libwebapi.rmeta" --extern js="$dir/libjs.rmeta" "${cfg[@]}" -o "$dir/$(basename "$out")" "$@"
