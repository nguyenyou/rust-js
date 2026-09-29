// What the crates tests share: the toolchain, Cargo for rust-js's target,
// and what a module's `main` prints under Node.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { checkCargo } from "../tooling/cargo.js";
import { node } from "./programs";
import { compiler, root, run } from "./support";

export const pin = readFileSync(join(root, "rust-toolchain.toml"), "utf8").match(/channel = "([^"]+)"/)![1];

export const cargo = (manifest: string, ...args: string[]) => ["cargo", `+${pin}`, ...args, "--offline", "--quiet", "--manifest-path", manifest];
/** `packageName` checked for rust-js's target, with rust-js as Cargo's workspace wrapper. */
export const check = (manifestPath: string, options: { packageName?: string, features?: string[], react?: string } = {}) =>
  checkCargo({ manifestPath, toolchain: pin, compiler, offline: true, packageName: "frontend", ...options });

export const printed = (app: string) => run([node ?? "node", "--input-type=module", "--eval", `(await import(${JSON.stringify(app)})).main();`]);
