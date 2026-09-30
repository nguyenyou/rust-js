// The binding crates as crates.io has them (ADR 0115): packaged together,
// each at the compiler's version, and each built from its package alone,
// by a plain stable rustc, as a registry's crate is.

import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { runSync } from "./child";
import { fixture, root } from "./support";

const version = (Bun.TOML.parse(readFileSync(join(root, "Cargo.toml"), "utf8")) as { package: { version: string } }).package.version;

test("the binding crates package for crates.io, at the compiler's version", () => {
  const out = fixture("crates-io");
  const packaged = runSync([process.execPath, "scripts/package-crates.ts", out], root, 600_000, { RUSTC_BOOTSTRAP: undefined });
  expect(packaged.stderr).not.toContain("error");
  expect(packaged.code).toBe(0);
  const crates = ["rust-js-builtins", "rust-js-webapi", "rust-js-react"].map((name) => `${name}-${version}.crate`);
  expect(readdirSync(out).sort()).toEqual([...crates].sort());
  // Each is its Rust, and what react's build script reads: not the
  // repository's tools that generate or build it.
  const listed = (name: string) =>
    runSync(["tar", "-tzf", join(out, `${name}-${version}.crate`)], root, 60_000)
      .stdout.trim().split("\n")
      .map((file) => file.slice(`${name}-${version}/`.length))
      .filter((file) => !["Cargo.lock", "Cargo.toml", "Cargo.toml.orig", ".cargo_vcs_info.json"].includes(file))
      .sort();
  expect(listed("rust-js-builtins")).toEqual(["README.md", "src/lib.rs"]);
  expect(listed("rust-js-webapi")).toEqual(["README.md", "src/lib.rs"]);
  expect(listed("rust-js-react")).toEqual(["README.md", "build.rs", "src/dom.rs", "src/elements.rs", "src/event.rs", "src/lib.rs", "versions.json"]);
}, 600_000);
