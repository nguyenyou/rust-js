// Fetches rustc's own UI tests, `tests/ui`, at the exact commit of the pinned
// toolchain, into `target/rustc-tests/<commit>/`, and prints that directory.
// Only that directory is downloaded: a shallow, sparse, blobless checkout.
//
//   bun scripts/rustc-tests.ts

import { existsSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");

function run(cmd: string[], cwd = root): string {
  const p = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`${cmd.join(" ")} failed:\n${p.stderr.toString()}`);
  return p.stdout.toString();
}

/** The commit the pinned rustc was built from, as the WASM build finds it. */
export function rustcCommit(): string {
  return run([process.execPath, join(root, "scripts", "toolchain.ts"), "commit"]).trim();
}

/** `tests/ui` at the pinned commit, fetched if it isn't here yet. */
export function rustcTests(): string {
  const commit = rustcCommit();
  const dir = join(root, "target", "rustc-tests", commit);
  const ui = join(dir, "tests", "ui");
  if (existsSync(join(dir, ".fetched"))) return ui;
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  run(["git", "init", "--quiet"], dir);
  run(["git", "remote", "add", "origin", "https://github.com/rust-lang/rust.git"], dir);
  run(["git", "sparse-checkout", "set", "--no-cone", "/tests/ui/"], dir);
  run(["git", "fetch", "--quiet", "--depth=1", "--filter=blob:none", "origin", commit], dir);
  run(["git", "checkout", "--quiet", "FETCH_HEAD"], dir);
  Bun.write(join(dir, ".fetched"), `${commit}\n`);
  return ui;
}

if (import.meta.main) console.log(rustcTests());
