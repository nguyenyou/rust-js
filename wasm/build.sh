#!/usr/bin/env bash
# Build rust-js, with rustc's front end, as a WASI program:
#   wasm/target/wasm32-wasip1/release/rust-js.wasm
# and stage the sysroot it type-checks against (wasm/sysroot).
# To let the deploy workflow skip this build, run `prebuilt.sh publish` after.
set -euo pipefail
cd "$(dirname "$0")"

TOOLCHAIN=nightly-2026-03-25
COMMIT=362211dc29abc4e8f8cfc384740237f144929b03

# 1. rustc's source: a worktree of a rust-lang/rust clone, at the pinned commit.
if [ ! -d rustc/compiler ]; then
  echo "error: ./rustc is missing. From a rust-lang/rust clone, run:" >&2
  echo "  git worktree add --no-checkout $PWD/rustc $COMMIT" >&2
  echo "  git -C $PWD/rustc sparse-checkout set --cone compiler library/proc_macro" >&2
  echo "  git -C $PWD/rustc checkout" >&2
  exit 1
fi

# 2. Our patches to rustc. Skip ones already applied; fail loudly on conflicts.
for patch in patches/*.patch; do
  if git -C rustc apply --reverse --check "../$patch" 2>/dev/null; then
    echo "already applied: $patch"
  else
    git -C rustc apply "../$patch"
    echo "applied: $patch"
  fi
done

# 3. psm (under rustc's `stacker`) bundles a precompiled wasm32.o using `ar`.
#    Apple's ar can't read Wasm objects and silently makes an empty archive,
#    which leaves `rust_psm_on_stack` undefined. Use LLVM's ar from the toolchain.
SYSROOT=$(rustc "+$TOOLCHAIN" --print sysroot)
HOST=$(rustc "+$TOOLCHAIN" -vV | sed -n 's/^host: //p')
export AR_wasm32_wasip1="$SYSROOT/lib/rustlib/$HOST/bin/llvm-ar"

cargo "+$TOOLCHAIN" build --release

# 4. Record which committed inputs this build came from (`dirty` if there were
#    uncommitted changes), so `prebuilt.sh publish` can refuse a stale binary.
./prebuilt.sh stamp

# 5. The sysroot rust-js type-checks against.
./stage-sysroot.sh

echo "built: $PWD/target/wasm32-wasip1/release/rust-js.wasm"
