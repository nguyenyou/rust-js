// A Cargo build's JS in source (ADR 0101): each module's beside its Rust,
// as ReScript writes it and a project commits it (ADR 0041).

import { beforeAll, expect, test } from "bun:test";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { checkCargo } from "../tooling/cargo.js";
import { node } from "./programs";
import { buildCompiler, compiler, fixture, root, run } from "./support";
import { pin } from "./crates";

beforeAll(buildCompiler, 600_000);

// An inline module's JS is where its file would be, `mod inner { .. }` of
// `src/lib.rs` beside it as `src/inner.js`, and one in `src/api.rs` in
// `src/api/`: in the parent's file, one would overwrite the other.
test("a Cargo build's JS in source has each inline module where its file would be", async () => {
  const dir = fixture("cargo-in-source-inline");
  const src = join(dir, "src");
  mkdirSync(src, { recursive: true });
  writeFileSync(join(dir, "Cargo.toml"), '[package]\nname = "app"\nversion = "0.1.0"\nedition = "2024"\n');
  writeFileSync(join(src, "lib.rs"), "mod api;\n\npub mod inner {\n    pub fn value() -> u32 {\n        42\n    }\n}\n\npub fn answer() -> u32 {\n    inner::value() + api::nested::one()\n}\n");
  writeFileSync(join(src, "api.rs"), "pub mod nested {\n    pub fn one() -> u32 {\n        1\n    }\n}\n");
  const { js, files } = await checkCargo({ manifestPath: join(dir, "Cargo.toml"), toolchain: pin, compiler, offline: true, packageName: "app", inSource: true });
  expect(js).toBe(join(src, "lib.js"));
  // `api` has nothing of its own, so no JS: `nested` is still in `src/api/`.
  expect([...files].sort()).toEqual([join(src, "api", "nested.js"), join(src, "inner.js"), join(src, "lib.js")]);
  expect(run([node ?? "node", "--input-type=module", "--eval", `console.log((await import(${JSON.stringify(js)})).answer());`]).trim()).toBe("43");
  for (const file of files) expect(readFileSync(file, "utf8")).toContain(`//# sourceMappingURL=${basename(file)}.map`);
}, 600_000);

// A module where its file would be can be where the crate's root is:
// `mod root { .. }` of a `[lib] path = "src/root.rs"`. Neither is written
// over the other: it's an error, as rust-js's own `mod lib` of `lib.rs` is.
test("a Cargo build's JS in source refuses two modules for one file", async () => {
  const dir = fixture("cargo-in-source-collision");
  const src = join(dir, "src");
  mkdirSync(src, { recursive: true });
  writeFileSync(join(dir, "Cargo.toml"), '[package]\nname = "app"\nversion = "0.1.0"\nedition = "2024"\n\n[lib]\npath = "src/root.rs"\n');
  writeFileSync(join(src, "root.rs"), "pub mod root {\n    pub fn value() -> u32 {\n        42\n    }\n}\n\npub fn answer() -> u32 {\n    root::value()\n}\n");
  const inSource = () => checkCargo({ manifestPath: join(dir, "Cargo.toml"), toolchain: pin, compiler, offline: true, packageName: "app", inSource: true });
  await expect(inSource()).rejects.toThrow(/the crate root and module `root` would both be .*src\/root\.js/);
  expect(existsSync(join(src, "root.js"))).toBe(false);
}, 600_000);

// Two crates' modules can be where one file would be too: an inline
// `mod helper` of `sources/alpha.rs`'s and of `sources/beta.rs`'s.
test("a Cargo build's JS in source refuses two crates' modules for one file", async () => {
  const dir = fixture("cargo-in-source-crates-collision");
  mkdirSync(join(dir, "sources"), { recursive: true });
  writeFileSync(join(dir, "Cargo.toml"), '[workspace]\nmembers = ["alpha", "beta"]\nresolver = "2"\n');
  for (const [name, dependency] of [["alpha", 'beta = { path = "../beta" }\n'], ["beta", ""]]) {
    mkdirSync(join(dir, name));
    writeFileSync(join(dir, name, "Cargo.toml"), `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2024"\n\n[lib]\npath = "../sources/${name}.rs"\n\n[dependencies]\n${dependency}`);
    const answer = name === "alpha" ? "helper::n() + beta::answer()" : "helper::n()";
    writeFileSync(join(dir, "sources", `${name}.rs`), `mod helper {\n    pub fn n() -> u32 {\n        1\n    }\n}\n\npub fn answer() -> u32 {\n    ${answer}\n}\n`);
  }
  const inSource = () => checkCargo({ manifestPath: join(dir, "Cargo.toml"), toolchain: pin, compiler, offline: true, packageName: "alpha", inSource: true });
  await expect(inSource()).rejects.toThrow(/crate `(alpha|beta)`'s module `helper` and crate `(alpha|beta)`'s module `helper` would both be .*sources\/helper\.js/);
  expect(readdirSync(join(dir, "sources")).sort()).toEqual(["alpha.rs", "beta.rs"]);
}, 600_000);

// The JS beside the Rust it's from, as ReScript writes it and a project
// commits it (ADR 0041): `src/api.rs` is `src/api.js`, importing the other
// crates' where they are too. What a module was, the module gone, goes.
test("a Cargo build's JS in source is beside each module's Rust, and follows the modules", async () => {
  const dir = fixture("cargo-in-source");
  writeFileSync(join(dir, "Cargo.toml"), '[workspace]\nmembers = ["app", "shared"]\nresolver = "2"\n');
  for (const name of ["app", "shared"]) mkdirSync(join(dir, name, "src"), { recursive: true });
  writeFileSync(join(dir, "shared", "Cargo.toml"), '[package]\nname = "shared"\nversion = "0.1.0"\nedition = "2024"\n');
  writeFileSync(join(dir, "shared", "src", "lib.rs"), "pub fn seven() -> u32 {\n    7\n}\n");
  writeFileSync(join(dir, "app", "Cargo.toml"), '[package]\nname = "app"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nshared = { path = "../shared" }\n');
  const app = join(dir, "app", "src");
  writeFileSync(join(app, "lib.rs"), "mod extra;\n\npub fn value() -> u32 {\n    extra::more(shared::seven())\n}\n");
  writeFileSync(join(app, "extra.rs"), "pub fn more(n: u32) -> u32 {\n    n + 1\n}\n");
  const manifest = join(dir, "Cargo.toml");
  const inSource = () => checkCargo({ manifestPath: manifest, toolchain: pin, compiler, offline: true, packageName: "app", inSource: true });
  const { js } = await inSource();
  expect(js).toBe(join(app, "lib.js"));
  expect(readFileSync(js, "utf8")).toContain('from "../../shared/src/lib.js"');
  expect(existsSync(join(app, "extra.js"))).toBe(true);
  expect(run([node ?? "node", "--input-type=module", "--eval", `console.log((await import(${JSON.stringify(js)})).value());`]).trim()).toBe("8");
  // The module gone, its JS goes; and the root, with JSX now, is `lib.jsx`.
  rmSync(join(app, "extra.rs"));
  writeFileSync(join(app, "lib.rs"), "#![allow(non_snake_case)]\n\nuse react::jsx;\n\npub fn value() -> u32 {\n    shared::seven()\n}\n\npub fn View() -> react::Element {\n    jsx! { <b /> }\n}\n");
  writeFileSync(join(dir, "app", "Cargo.toml"), readFileSync(join(dir, "app", "Cargo.toml"), "utf8") + `react = { package = "rust-js-react", path = ${JSON.stringify(join(root, "react"))} }\n`);
  const again = await inSource();
  expect(again.js).toBe(join(app, "lib.jsx"));
  expect(existsSync(join(app, "extra.js"))).toBe(false);
  expect(existsSync(join(app, "lib.js"))).toBe(false);
  // A file of the project's own, not rust-js's, stays.
  writeFileSync(join(app, "notes.js"), "export const notes = 1;\n");
  await inSource();
  expect(existsSync(join(app, "notes.js"))).toBe(true);
}, 600_000);