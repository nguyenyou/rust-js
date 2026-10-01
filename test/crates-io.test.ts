// The binding crates as crates.io has them (ADR 0115): packaged together,
// each at its own version, and each built from its package alone, by a
// plain stable rustc, as a registry's crate is. builtins and webapi have
// theirs, released on npm; react, for now, the compiler's.

import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { runSync } from "./child";
import { fixture, root } from "./support";

const versionOf = (dir: string) =>
  (Bun.TOML.parse(readFileSync(join(root, dir, "Cargo.toml"), "utf8")) as { package: { version: string } }).package.version;
const versions: Record<string, string> = {
  "rust-js-builtins": versionOf("builtins"),
  "rust-js-webapi": versionOf("webapi"),
  "rust-js-react": versionOf("react"),
};

test("the binding crates package for crates.io, each at its version", () => {
  const out = fixture("crates-io");
  const packaged = runSync([process.execPath, "scripts/package-crates.ts", out], root, 600_000, { RUSTC_BOOTSTRAP: undefined });
  expect(packaged.stderr).not.toContain("error");
  expect(packaged.code).toBe(0);
  const crates = ["rust-js-builtins", "rust-js-webapi", "rust-js-react"].map((name) => `${name}-${versions[name]}.crate`);
  expect(readdirSync(out).sort()).toEqual([...crates].sort());
  // Each is its Rust, and what react's build script reads: not the
  // repository's tools that generate or build it.
  const listed = (name: string) =>
    runSync(["tar", "-tzf", join(out, `${name}-${versions[name]}.crate`)], root, 60_000)
      .stdout.trim().split("\n")
      .map((file) => file.slice(`${name}-${versions[name]}/`.length))
      .filter((file) => !["Cargo.lock", "Cargo.toml", "Cargo.toml.orig", ".cargo_vcs_info.json"].includes(file))
      .sort();
  expect(listed("rust-js-builtins")).toEqual(["README.md", "src/lib.rs"]);
  expect(listed("rust-js-webapi")).toEqual(["README.md", "src/lib.rs"]);
  expect(versionOf("react")).toBe(versionOf("."));
  expect(listed("rust-js-react")).toEqual(["README.md", "build.rs", "src/dom.rs", "src/elements.rs", "src/event.rs", "src/lib.rs", "versions.json"]);
}, 600_000);
