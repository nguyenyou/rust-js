#!/usr/bin/env bash
# Run a command in a Linux VM on macOS, where a freshly built binary isn't
# scanned before its first run, as it is on macOS: tests that build many
# native programs run in minutes, not an hour. See AGENTS.md.
#
#   scripts/linux-vm.sh 'bun scripts/rustc-suite.ts drop'   one string: run as written
#   scripts/linux-vm.sh bun run test                        several words: each quoted
#
# The VM is a Tart VM, `rustjs` unless RUST_JS_VM names another. It works in
# its own copy of the sources, ~/rust-js, synced from this checkout before
# each command. What git ignores, target/, node_modules/, the WASM build's
# rustc checkout, is the VM's own, never this checkout's. What the command
# writes that git doesn't ignore, a blessed snapshot or list, is copied back
# to this checkout after it, whether it passed or failed.
set -euo pipefail

VM="${RUST_JS_VM:-rustjs}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! tart list | awk -v vm="$VM" '$2 == vm && $NF == "running"' | grep -q .; then
  nohup tart run "$VM" --no-graphics --dir=rust-js:"$ROOT" >"${TMPDIR:-/tmp}/rust-js-vm.log" 2>&1 &
  disown
  tart ip "$VM" --wait 180 >/dev/null
fi

if [ "$#" -eq 1 ]; then
  cmd="$1"
else
  cmd=$(printf '%q ' "$@")
fi

tart exec "$VM" bash -lc "
  set -e
  mountpoint -q /mnt/shared || sudo mount -t virtiofs com.apple.virtio-fs.automount /mnt/shared
  . \"\$HOME/.cargo/env\"
  export PATH=\"\$HOME/.bun/bin:\$PATH\" NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt
  rsync -a --delete --filter=':- .gitignore' /mnt/shared/rust-js/ \"\$HOME/rust-js/\"
  cd \"\$HOME/rust-js\"
  touch /tmp/rust-js-synced
  status=0
  ( $cmd ) || status=\$?
  git ls-files -z --cached --others --exclude-standard |
    while IFS= read -r -d '' file; do [ \"\$file\" -nt /tmp/rust-js-synced ] && printf '%s\\0' \"\$file\"; done |
    rsync -rtp --omit-dir-times --from0 --files-from=- ./ /mnt/shared/rust-js/
  # Not \`exit\`, which would run the login shell's logout script.
  (exit \$status)
"
