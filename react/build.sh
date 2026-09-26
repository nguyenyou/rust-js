#!/usr/bin/env bash
# Compile the react crate's metadata (ADR 0041), and the web crate's beside
# it, which it uses:
#
#   react/build.sh -o target/libreact.rmeta      # writes target/libweb.rmeta too
#
# Then compile a program with
# `rust-js app.rs -- --extern react=target/libreact.rmeta -L target`.
set -euo pipefail
[ "${1:-}" = "-o" ] && [ -n "${2:-}" ] || { echo "usage: react/build.sh -o <dir>/libreact.rmeta [rustc flags]" >&2; exit 1; }
out=$2
shift 2
dir=$(cd "$(dirname "$out")" && pwd)
cd "$(dirname "$0")"
../web/build.sh -o "$dir/libweb.rmeta" "$@"
exec rustc --edition=2024 --crate-type=lib --crate-name=react --emit=metadata src/lib.rs \
  --extern web="$dir/libweb.rmeta" -o "$dir/$(basename "$out")" "$@"
