// The same crates as a Cargo workspace (ADR 0101), built by Cargo with
// rust-js as its workspace wrapper: each library of the workspace compiled by
// rust-js, dependencies first, and serde, a crate of crates.io, by rustc, as
// Cargo would. `check`, a native binary calling `frontend::main`, is the oracle.

import { beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { node } from "./programs";
import { buildCompiler, buildSerde, compiler, fixture, root, run } from "./support";
import { cargo, check, printed } from "./crates";

beforeAll(buildCompiler, 600_000);

// serde and serde_json, as the metadata rust-js reads.
const serde = { rmeta: () => buildSerde("rmeta") };

const members: Record<string, string> = {
  validation: "",
  models: 'validation = { path = "../validation" }\nserde = { version = "1", features = ["derive"] }\n',
  frontend: 'models = { path = "../models" }\nvalidation = { path = "../validation" }\nserde_json = "1"\n',
};

/** The workspace, with the lockfile serde/build.sh's serde was built from. */
function workspace(dir: string): string {
  writeFileSync(join(dir, "Cargo.toml"), '[workspace]\nmembers = ["validation", "models", "frontend", "shell", "check"]\nresolver = "2"\n');
  cpSync(join(root, "serde", "Cargo.lock"), join(dir, "Cargo.lock"));
  for (const [name, uses] of Object.entries(members)) {
    mkdirSync(join(dir, name, "src"), { recursive: true });
    writeFileSync(join(dir, name, "Cargo.toml"), `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\n${uses}`);
    cpSync(join(root, "test", "crates", name, "lib.rs"), join(dir, name, "src", "lib.rs"));
  }
  // Using `frontend` only, and through it the rest.
  mkdirSync(join(dir, "shell", "src"), { recursive: true });
  writeFileSync(join(dir, "shell", "Cargo.toml"), '[package]\nname = "shell"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nfrontend = { path = "../frontend" }\n');
  writeFileSync(join(dir, "shell", "src", "lib.rs"), "pub fn main() {\n    frontend::main();\n}\n");
  mkdirSync(join(dir, "check", "src"), { recursive: true });
  writeFileSync(join(dir, "check", "Cargo.toml"), '[package]\nname = "check"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nfrontend = { path = "../frontend" }\n');
  writeFileSync(join(dir, "check", "src", "main.rs"), "fn main() {\n    frontend::main();\n}\n");
  return join(dir, "Cargo.toml");
}


test("a Cargo workspace built with rust-js as its wrapper prints what native Rust does, and follows an edit", async () => {
  serde.rmeta();
  const dir = fixture("crates-cargo");
  const manifest = workspace(dir);
  const native = () => run(cargo(manifest, "run", "-p", "check"));
  // `models` built first on its own, the package Cargo was asked for, then used.
  await check(manifest, { packageName: "models" });
  const { js, crates } = await check(manifest);
  expect([...crates.keys()].sort()).toEqual(["frontend", "models", "validation"]);
  const before = printed(js);
  expect(before).toBe(native());
  const validation = join(dir, "validation", "src", "lib.rs");
  writeFileSync(validation, readFileSync(validation, "utf8").replace("isn't an email address", "is no email address"));
  const after = printed((await check(manifest)).js);
  expect(after).toBe(native());
  expect(after).not.toBe(before);
  // A crate told of the libraries its dependency was compiled against.
  expect(printed((await check(manifest, { packageName: "shell" })).js)).toBe(after);
}, 600_000);

// What Cargo has as done is what rust-js made (ADR 0101): Cargo rebuilds a
// crate when what its record of the sources lists changes, which rust-js adds
// itself to, and a crate's JS is beside its metadata.
test("a Cargo build of rust-js's crates is done when their JS is, and not before", async () => {
  serde.rmeta();
  const dir = fixture("crates-cargo-fresh");
  const manifest = workspace(dir);
  const { js, crates } = await check(manifest);
  const written = statSync(js).mtimeMs;
  expect((await check(manifest)).js).toBe(js);
  expect(statSync(js).mtimeMs).toBe(written);
  const deps = join(dir, "target", "wasm32-unknown-unknown", "debug", "deps");
  expect(dirname(dirname(js))).toBe(join(deps, "rust-js"));
  const recorded = readdirSync(deps).filter((f) => f.startsWith("validation-") && f.endsWith(".d")).map((f) => readFileSync(join(deps, f), "utf8"));
  expect(recorded.length).toBe(1);
  // The compiler that ran, which a change to makes Cargo build again: an
  // installed launcher's is the binary beside it, which it runs.
  const launcher = readFileSync(compiler).subarray(0, 2).toString() === "#!";
  const binary = launcher ? join(dirname(realpathSync(compiler)), "compiler") : compiler;
  // A dep-info file's last path, its spaces escaped, `\ `.
  const named = recorded[0].split("\n")[0].split(/(?<!\\) /).at(-1)!.replaceAll("\\ ", " ");
  expect(realpathSync(named)).toBe(realpathSync(binary));
  // A library's JS gone from a build Cargo has as done: refused, not given to
  // an app that imports it, until Cargo is told to build it again. Found in
  // review.
  rmSync(crates.get("models")!.js);
  await expect(check(manifest)).rejects.toThrow("cargo clean -p models --target wasm32-unknown-unknown");
  run(cargo(manifest, "clean", "-p", "models", "--target", "wasm32-unknown-unknown"));
  expect(printed((await check(manifest)).js)).toBe(printed(js));
  // A library's manifest gone, the crates using it are refused, not compiled
  // as if it were a crate rustc built.
  rmSync(dirname(crates.get("validation")!.manifest), { recursive: true });
  const models = join(dir, "models", "src", "lib.rs");
  writeFileSync(models, readFileSync(models, "utf8") + "\npub fn more() {}\n");
  await expect(check(manifest)).rejects.toThrow("`cargo clean` to build it again");
  expect(() => run(cargo(manifest, "build", "--target", "wasm32-unknown-unknown", "-p", "validation"), 600_000, { RUSTC_WORKSPACE_WRAPPER: compiler }))
    .toThrow("`cargo build` asks for what rustc links");
}, 600_000);

/** A workspace of one library, `app`, whose `value` its feature `alternate` changes. */
function featured(dir: string): string {
  writeFileSync(join(dir, "Cargo.toml"), '[workspace]\nmembers = ["app"]\nresolver = "2"\n');
  mkdirSync(join(dir, "app", "src"), { recursive: true });
  writeFileSync(join(dir, "app", "Cargo.toml"), '[package]\nname = "app"\nversion = "0.1.0"\nedition = "2024"\n\n[features]\nalternate = []\n');
  writeFileSync(join(dir, "app", "src", "lib.rs"),
    '#[cfg(not(feature = "alternate"))]\npub fn value() -> u32 {\n    1\n}\n#[cfg(feature = "alternate")]\npub fn value() -> u32 {\n    2\n}\n');
  return join(dir, "Cargo.toml");
}
const value = (js: string) => run([node ?? "node", "--input-type=module", "--eval", `console.log((await import(${JSON.stringify(js)})).value());`]).trim();

// Found in review: each feature set's build wrote one place, and Cargo, with
// the first's cached, had the second's JS as the first's.
test("a Cargo build of another feature set, and back, is each one's JS", async () => {
  const manifest = featured(fixture("cargo-features"));
  const values = [];
  for (const features of [[], ["alternate"], []]) values.push(value((await check(manifest, { packageName: "app", features })).js));
  expect(values).toEqual(["1", "2", "1"]);
}, 600_000);

// Found in review: a crate rustc checked, fresh when rust-js was asked for.
test("a crate Cargo checked without rust-js is compiled by it once it's the wrapper", async () => {
  const manifest = featured(fixture("cargo-wrapper-added"));
  run(cargo(manifest, "check", "--target", "wasm32-unknown-unknown"));
  expect(value((await check(manifest, { packageName: "app" })).js)).toBe("1");
}, 600_000);

// Found in review: what Cargo is told of a crate was written after its JS was
// published, so a build that failed writing it had changed the JS.
test("a Cargo build that can't record what it made leaves the previous build's JS", async () => {
  const dir = fixture("cargo-record-blocked");
  const manifest = featured(dir);
  const { js } = await check(manifest, { packageName: "app" });
  const before = readFileSync(js, "utf8");
  const deps = join(dir, "target", "wasm32-unknown-unknown", "debug", "deps");
  const marker = join(deps, readdirSync(deps).find((f) => f.endsWith(".rust-js"))!);
  rmSync(marker);
  mkdirSync(join(marker, "in the way"), { recursive: true });
  const lib = join(dir, "app", "src", "lib.rs");
  writeFileSync(lib, readFileSync(lib, "utf8").replace("    1\n", "    3\n"));
  await expect(check(manifest, { packageName: "app" })).rejects.toThrow();
  expect(readFileSync(js, "utf8")).toBe(before);
}, 600_000);