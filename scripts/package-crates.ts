// Package the binding crates for crates.io (ADR 0115), or with `--publish`,
// publish them. Each is staged as git has it, in a workspace of the three,
// so Cargo packages them together, the ones a crate depends on from what it
// packaged, and builds each from its package alone, with the pinned stable
// rustc and no `RUSTC_BOOTSTRAP`, as a user's `cargo check` would.
//
//   bun scripts/package-crates.ts <out-dir>   # package and verify them
//   bun scripts/package-crates.ts --publish   # publish them, after `cargo login`

import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const args = Bun.argv.slice(2);
const publish = args.length === 1 && args[0] === "--publish";
if (!publish && (args.length !== 1 || args[0].startsWith("-"))) throw new Error("Usage: bun scripts/package-crates.ts <out-dir> | --publish");
const crates = ["builtins", "webapi", "react"];
const { channel } = (Bun.TOML.parse(readFileSync(join(root, "rust-toolchain.toml"), "utf8")) as { toolchain: { channel: string } }).toolchain;
const { RUSTC_BOOTSTRAP: _, ...env } = process.env;

const run = (cmd: string[], cwd: string) => {
  const p = Bun.spawnSync(cmd, { cwd, env, stdout: "inherit", stderr: "inherit" });
  if (p.exitCode !== 0) throw new Error(`${cmd.join(" ")} failed`);
};

const staging = mkdtempSync(join(tmpdir(), "rust-js-crates-"));
try {
  for (const crate of crates) {
    const listed = Bun.spawnSync(["git", "ls-files", crate], { cwd: root }).stdout.toString().trim().split("\n");
    for (const file of listed) {
      mkdirSync(dirname(join(staging, file)), { recursive: true });
      copyFileSync(join(root, file), join(staging, file));
    }
  }
  writeFileSync(join(staging, "Cargo.toml"), `[workspace]\nmembers = ${JSON.stringify(crates)}\nresolver = "3"\n`);
  if (publish) run(["cargo", `+${channel}`, "publish", "--workspace"], staging);
  else {
    run(["cargo", `+${channel}`, "package", "--workspace", "--offline"], staging);
    const out = resolve(args[0]);
    mkdirSync(out, { recursive: true });
    const packaged = join(staging, "target", "package");
    for (const file of readdirSync(packaged).filter((file) => file.endsWith(".crate"))) copyFileSync(join(packaged, file), join(out, file));
  }
} finally {
  rmSync(staging, { recursive: true, force: true });
}
