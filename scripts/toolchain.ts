// One toolchain pin for native builds, WASM source, and deployment.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const config = Bun.TOML.parse(readFileSync(resolve(root, "rust-toolchain.toml"), "utf8"));
const channel = config.toolchain?.channel;
if (typeof channel !== "string" || !channel) throw new Error("rust-toolchain.toml must declare a channel");

function run(args: string[]): string {
  const result = Bun.spawnSync(args, { cwd: root, stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error(result.stderr.toString().trim() || `${args[0]} failed`);
  return result.stdout.toString().trim();
}

try {
  const [command, path, ...extra] = Bun.argv.slice(2);
  if (extra.length || (command !== "check-source" && path)) throw new Error("Unexpected toolchain arguments");
  if (command === "channel") {
    console.log(channel);
  } else if (command === "commit" || command === "check-source") {
    const version = run(["rustc", `+${channel}`, "-vV"]);
    const commit = version.match(/^commit-hash: ([0-9a-f]{40})$/m)?.[1];
    if (!commit) throw new Error(`Cannot identify rustc source commit for ${channel}`);
    if (command === "commit") console.log(commit);
    else {
      if (!path) throw new Error("check-source requires a rustc checkout path");
      const actual = run(["git", "-C", resolve(path), "rev-parse", "HEAD"]);
      if (actual !== commit) throw new Error(`rustc source mismatch: ${channel} requires ${commit}, checkout is ${actual}. Use a checkout at the pinned commit before building WASM.`);
    }
  } else throw new Error("Usage: bun scripts/toolchain.ts channel | commit | check-source <path>");
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
