#!/usr/bin/env bash
# A prebuilt rust-js.wasm, kept as a GitHub Release asset, so the deploy
# workflow can skip compiling rustc's front end (many minutes on CI).
#
#   prebuilt.sh hash      print the hash of the inputs that determine rust-js.wasm
#   prebuilt.sh publish   upload the local build as release `wasm-<hash>`,
#                         then delete the previous `wasm-*` releases
#   prebuilt.sh fetch     download the build for the current inputs, if published
#   prebuilt.sh stamp     (build.sh) record which inputs the local build came from
#
# The hash covers committed content only (`git ls-tree`), so it's the same on
# any machine for the same commit. A binary can only be published for the
# exact, pushed inputs it was built from, so CI never deploys a stale one:
# if nothing matches, it builds from source instead. That's also why only
# the newest release is kept: deploying an older commit just builds it.
set -euo pipefail
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"

WASM=wasm/target/wasm32-wasip1/release/rust-js.wasm
STAMP=$WASM.inputs
# Everything that can change the binary. The toolchain and the rustc commit
# are pinned inside build.sh, so they're covered too.
INPUTS=(src wasm/Cargo.toml wasm/Cargo.lock wasm/.cargo wasm/patches wasm/build.sh)

die() {
  echo "prebuilt.sh: $*" >&2
  exit 1
}

inputs_hash() {
  git ls-tree -r HEAD -- "${INPUTS[@]}" | git hash-object --stdin | cut -c1-16
}

# No uncommitted or untracked changes to the inputs.
inputs_clean() {
  git diff --quiet HEAD -- "${INPUTS[@]}" &&
    [ -z "$(git ls-files --others --exclude-standard -- "${INPUTS[@]}")" ]
}

case "${1:-}" in
  hash)
    inputs_hash
    ;;
  stamp)
    if inputs_clean; then inputs_hash > "$STAMP"; else echo dirty > "$STAMP"; fi
    ;;
  publish)
    hash=$(inputs_hash)
    inputs_clean || die "commit these first: ${INPUTS[*]}"
    [ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$hash" ] ||
      die "$WASM wasn't built from the committed inputs; run wasm/build.sh"
    git branch -r --contains HEAD | grep -q . || die "push HEAD first: the release points at it"
    tag="wasm-$hash"
    if ! gh release view "$tag" >/dev/null 2>&1; then
      gh release create "$tag" --target "$(git rev-parse HEAD)" --prerelease \
        --title "rust-js.wasm $hash" \
        --notes "rust-js.wasm built by \`wasm/build.sh\` from $(git rev-parse --short HEAD). The *Deploy playground* workflow downloads it instead of building. \`wasm/prebuilt.sh publish\` deletes it when a newer one is published; the workflow then builds older commits from source."
    fi
    gh release upload "$tag" "$WASM" --clobber
    echo "published: $tag"
    # Now that the new one is up, delete the ones before it (only ours: `wasm-*`).
    gh release list --limit 100 --json tagName \
      --jq ".[].tagName | select(startswith(\"wasm-\") and . != \"$tag\")" |
      while read -r previous; do
        gh release delete "$previous" --cleanup-tag --yes
        echo "deleted previous: $previous"
      done
    ;;
  fetch)
    tag="wasm-$(inputs_hash)"
    mkdir -p "$(dirname "$WASM")"
    if gh release download "$tag" --pattern rust-js.wasm --dir "$(dirname "$WASM")" --clobber 2>/dev/null; then
      echo "downloaded: $tag"
    else
      echo "no prebuilt rust-js.wasm for $tag"
      exit 1
    fi
    ;;
  *)
    die "usage: prebuilt.sh hash | publish | fetch | stamp"
    ;;
esac
