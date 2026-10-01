// The patch an app's Cargo needs to find the crates its npm packages have:
// each package with `"rust-js": { "crate" }`, where its package manager put
// it, npm's or bun's way, all in one `node_modules`, or pnpm's, each in a
// store, and linked. Written into the app's `.cargo/config.toml`, between
// rust-js's lines, so Cargo, and an editor's rust-analyzer, find them.

import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { runSync } from "./child";
import { fixture, root } from "./support";

type Crate = { npm: string; crate: string; lib: string; version: string; peers?: Record<string, string>; deps?: string; source: string };

// `a` has a type; `b` takes it, so an app that passes one of a's to b's has
// one `a`, or doesn't compile.
const a = (version = "0.0.1"): Crate => ({ npm: "@x/a", crate: "zz-a", lib: "a", version, source: "pub struct Thing;\n" });
const b: Crate = {
  npm: "@x/b",
  crate: "zz-b",
  lib: "b",
  version: "0.0.1",
  peers: { "@x/a": "~0.0.1" },
  deps: `a = { package = "zz-a", version = "~0.0.1" }\n`,
  source: "pub fn take(_: &a::Thing) {}\n",
};

/** `crate` as its npm package, at `dir`. */
function install(dir: string, crate: Crate, cargoVersion = crate.version) {
  mkdirSync(join(dir, "src"), { recursive: true });
  writeFileSync(
    join(dir, "package.json"),
    JSON.stringify({ name: crate.npm, version: crate.version, "rust-js": { crate: crate.crate }, peerDependencies: crate.peers }),
  );
  writeFileSync(
    join(dir, "Cargo.toml"),
    `[package]\nname = "${crate.crate}"\nversion = "${cargoVersion}"\nedition = "2024"\n\n[lib]\nname = "${crate.lib}"\n\n[dependencies]\n${crate.deps ?? ""}`,
  );
  writeFileSync(join(dir, "src", "lib.rs"), crate.source);
}

/** An app that depends on `a` and `b`, in npm and in Cargo, and uses both. */
function app(name: string) {
  const dir = fixture(name);
  mkdirSync(join(dir, "src"));
  writeFileSync(join(dir, "package.json"), JSON.stringify({ private: true, dependencies: { "@x/a": "~0.0.1", "@x/b": "~0.0.1" } }));
  writeFileSync(
    join(dir, "Cargo.toml"),
    `[package]\nname = "app"\nversion = "0.0.0"\nedition = "2024"\n\n[dependencies]\na = { package = "zz-a", version = "~0.0.1" }\nb = { package = "zz-b", version = "~0.0.1" }\n\n[workspace]\n`,
  );
  writeFileSync(join(dir, "src", "lib.rs"), "pub fn f() {\n    b::take(&a::Thing);\n}\n");
  return dir;
}

const patch = (dir: string) => runSync([process.execPath, join(root, "tooling", "patch.js"), dir], dir, 60_000);
const config = (dir: string) => readFileSync(join(dir, ".cargo", "config.toml"), "utf8");
const check = (dir: string) => runSync(["cargo", "check", "--offline", "--quiet", "--manifest-path", join(dir, "Cargo.toml")], dir, 300_000, { RUSTC_BOOTSTRAP: undefined });

const block = (lines: string[]) =>
  `# rust-js: begin\n# Where Cargo finds each crate an npm package has: written by rust-js from\n# what's installed, as each install ends. Edits here are written over.\n[patch.crates-io]\n${lines.join("\n")}\n# rust-js: end\n`;

test("an app's patch names each crate its packages have, where npm put it, and Cargo finds them", () => {
  const dir = app("patch-flat");
  install(join(dir, "node_modules", "@x", "a"), a());
  install(join(dir, "node_modules", "@x", "b"), b);
  const written = patch(dir);
  expect(written.stderr).toBe("");
  expect(written.code).toBe(0);
  expect(config(dir)).toBe(
    block(['zz-a = { path = "node_modules/@x/a" }', 'zz-b = { path = "node_modules/@x/b" }']),
  );
  const checked = check(dir);
  expect(checked.stderr).toBe("");
  expect(checked.code).toBe(0);
  // Written again, as it is.
  expect(patch(dir).code).toBe(0);
  expect(config(dir)).toBe(block(['zz-a = { path = "node_modules/@x/a" }', 'zz-b = { path = "node_modules/@x/b" }']));
}, 300_000);

test("pnpm's packages are where its store has them, and a peer is the one linked beside its package", () => {
  const dir = app("patch-pnpm");
  const store = join(dir, "node_modules", ".pnpm");
  const aDir = join(store, "@x+a@0.0.1", "node_modules", "@x", "a");
  const bDir = join(store, "@x+b@0.0.1_@x+a@0.0.1", "node_modules", "@x", "b");
  install(aDir, a());
  install(bDir, b);
  const link = (target: string, at: string) => {
    mkdirSync(dirname(at), { recursive: true });
    symlinkSync(relative(dirname(at), target), at);
  };
  link(aDir, join(dirname(bDir), "a"));
  link(aDir, join(dir, "node_modules", "@x", "a"));
  link(bDir, join(dir, "node_modules", "@x", "b"));
  const written = patch(dir);
  expect(written.stderr).toBe("");
  expect(written.code).toBe(0);
  expect(config(dir)).toBe(
    block([
      'zz-a = { path = "node_modules/.pnpm/@x+a@0.0.1/node_modules/@x/a" }',
      'zz-b = { path = "node_modules/.pnpm/@x+b@0.0.1_@x+a@0.0.1/node_modules/@x/b" }',
    ]),
  );
  const checked = check(dir);
  expect(checked.stderr).toBe("");
  expect(checked.code).toBe(0);
}, 300_000);

// Two of one crate would be two of each of its types, which Cargo would
// build, and an app that passes one to what takes the other wouldn't
// compile: a patch names one, so it's refused, naming who asked for each.
test("a crate installed twice is refused, with who asked for each", () => {
  const dir = app("patch-twice");
  install(join(dir, "node_modules", "@x", "a"), a());
  install(join(dir, "node_modules", "@x", "b"), b);
  install(join(dir, "node_modules", "@x", "b", "node_modules", "@x", "a"), a("0.0.2"));
  const written = patch(dir);
  expect(written.code).not.toBe(0);
  expect(written.stderr).toContain("zz-a is installed twice");
  expect(written.stderr).toContain("0.0.1 at node_modules/@x/a, for the app");
  expect(written.stderr).toContain("0.0.2 at node_modules/@x/b/node_modules/@x/a, for @x/b");
  expect(existsSync(join(dir, ".cargo", "config.toml"))).toBe(false);
});

test("a package whose crate is another version than it is refused", () => {
  const dir = app("patch-versions");
  install(join(dir, "node_modules", "@x", "a"), a(), "0.0.2");
  install(join(dir, "node_modules", "@x", "b"), b);
  const written = patch(dir);
  expect(written.code).not.toBe(0);
  expect(written.stderr).toContain("@x/a is 0.0.1, and its crate, zz-a, 0.0.2");
});

test("what else the app's config has is kept, and a patch of its own refused", () => {
  const dir = app("patch-config");
  install(join(dir, "node_modules", "@x", "a"), a());
  install(join(dir, "node_modules", "@x", "b"), b);
  mkdirSync(join(dir, ".cargo"));
  writeFileSync(join(dir, ".cargo", "config.toml"), "[build]\njobs = 2\n");
  expect(patch(dir).code).toBe(0);
  const lines = ['zz-a = { path = "node_modules/@x/a" }', 'zz-b = { path = "node_modules/@x/b" }'];
  expect(config(dir)).toBe(`[build]\njobs = 2\n\n${block(lines)}`);
  expect(patch(dir).code).toBe(0);
  expect(config(dir)).toBe(`[build]\njobs = 2\n\n${block(lines)}`);

  writeFileSync(join(dir, ".cargo", "config.toml"), '[patch.crates-io]\nserde = { path = "../serde" }\n');
  const refused = patch(dir);
  expect(refused.code).not.toBe(0);
  expect(refused.stderr).toContain("[patch.crates-io]");
});

test("an app with no crate in its packages has no patch", () => {
  const dir = fixture("patch-none");
  writeFileSync(join(dir, "package.json"), JSON.stringify({ private: true, dependencies: { left: "1" } }));
  mkdirSync(join(dir, "node_modules", "left"), { recursive: true });
  writeFileSync(join(dir, "node_modules", "left", "package.json"), JSON.stringify({ name: "left", version: "1.0.0" }));
  const written = patch(dir);
  expect(written.code).toBe(0);
  expect(existsSync(join(dir, ".cargo", "config.toml"))).toBe(false);
});
