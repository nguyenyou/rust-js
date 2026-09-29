// Separate crates (ADR 0100): each compiled once, by rust-js, to JS of its
// own and the metadata rustc reads of it, and a crate using it told what it
// needs by the manifest beside them.
//
//   test/crates/validation ◄── models ◄── frontend
//
// Natively, the same crates are rlibs, and `main` a program's. The JS
// must print what the native program does.

import { beforeAll, describe, expect, test } from "bun:test";
import { chmodSync, cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";

import { cfgFlags } from "../react/cfg.js";
import { checkCargo } from "../tooling/cargo.js";
import { bindingInputs } from "../tooling/resources.js";
import { node } from "./programs";
import { buildCompiler, buildReact, buildSerde, compiler, fixture, root, run, target } from "./support";

beforeAll(buildCompiler, 600_000);

// serde and serde_json, which `models` derives and `frontend` calls, as
// natively built libraries, and as the metadata rust-js reads.
const serde = { rlib: () => buildSerde("rlib"), rmeta: () => buildSerde("rmeta") };

// Each crate, and the ones it uses, dependencies first.
const graph: [string, string[]][] = [
  ["validation", []],
  ["models", ["validation"]],
  ["frontend", ["models", "validation"]],
];

/** The crates, copied to be edited. */
function sources(dir: string): string {
  const src = join(dir, "src");
  cpSync(join(root, "test", "crates"), src, { recursive: true });
  return src;
}

/** The native program's output: each crate an rlib, and `frontend::main` a binary's `main`. */
function native(dir: string, src: string): string {
  const out = join(dir, "native");
  mkdirSync(out, { recursive: true });
  const externs = (uses: string[]) => uses.flatMap((u) => ["--extern", `${u}=${join(out, `lib${u}.rlib`)}`]);
  for (const [name, uses] of graph) {
    run(["rustc", "--edition=2024", "-Coverflow-checks=off", "--crate-type=rlib", "--crate-name", name, join(src, name, "lib.rs"),
      "-o", join(out, `lib${name}.rlib`), ...externs(uses), "-L", `dependency=${out}`, ...serde.rlib()]);
  }
  const main = join(out, "main.rs");
  writeFileSync(main, "fn main() {\n    frontend::main();\n}\n");
  run(["rustc", "--edition=2024", "-Coverflow-checks=off", main, "-o", join(out, "program"), ...externs(["frontend"]), "-L", `dependency=${out}`, ...serde.rlib()]);
  return run([join(out, "program")]);
}

const js = (dir: string, name: string) => join(dir, "js", name);

/** The command rust-js compiles `name` with: a library writes its metadata too. */
function command(dir: string, src: string, name: string, uses: string[]): string[] {
  const externs = uses.flatMap((u) => ["--extern", `${u}=${join(js(dir, u), `lib${u}.rmeta`)}`]);
  const dependencies = uses.flatMap((u) => ["--dependency", join(js(dir, u), "lib.manifest.json")]);
  const library = name === "frontend" ? [] : ["--library"];
  const metadata = name === "frontend" ? [] : [`--emit=metadata=${join(js(dir, name), `lib${name}.rmeta`)}`];
  return [compiler, join(src, name, "lib.rs"), "-o", join(js(dir, name), "lib.js"), ...library, "--manifest", join(js(dir, name), "lib.manifest.json"),
    ...dependencies, "--", "--edition=2024", "--crate-name", name, ...metadata, ...externs, ...uses.flatMap((u) => ["-L", `dependency=${js(dir, u)}`]), ...serde.rmeta()];
}

/** Each crate compiled by rust-js on its own, dependencies first, into `<dir>/js/<crate>/`. */
function compile(dir: string, src: string): string {
  for (const [name, uses] of graph) run(command(dir, src, name, uses));
  return join(js(dir, "frontend"), "lib.js");
}

const printed = (app: string) => run([node ?? "node", "--input-type=module", "--eval", `(await import(${JSON.stringify(app)})).main();`]);

test("an app using two crates, each compiled on its own, prints what native Rust does", () => {
  const dir = fixture("crates");
  const src = sources(dir);
  const app = compile(dir, src);
  expect(printed(app)).toBe(native(dir, src));
  // Each crate imports the crates it uses, where they are.
  expect(readFileSync(join(js(dir, "models"), "lib.js"), "utf8")).toContain('from "../validation/lib.js"');
  expect(readFileSync(app, "utf8")).toContain('from "../models/lib.js"');
}, 300_000);

// A crate the others use, edited: what was made from the old one is
// refused, and once each is rebuilt in order, the app is the edited program.
test("an edited library is what its consumers use once they're rebuilt, and refused before", () => {
  const dir = fixture("crates-edited");
  const src = sources(dir);
  const app = compile(dir, src);
  const before = printed(app);
  const validation = join(src, "validation", "lib.rs");
  writeFileSync(validation, readFileSync(validation, "utf8").replace("isn't an email address", "is no email address"));
  run(command(dir, src, "validation", []));
  const previous = readFileSync(app, "utf8");
  // `models` was made from the old `validation`, which its manifest says.
  expect(() => run(command(dir, src, "frontend", ["models", "validation"]))).toThrow("dependency artifact changed");
  expect(readFileSync(app, "utf8")).toBe(previous);
  const after = printed(compile(dir, src));
  expect(after).toBe(native(dir, src));
  expect(after).not.toBe(before);
}, 300_000);

// A library's JS and the metadata its consumer's rustc reads must be one
// build's: the metadata its manifest lists is fingerprinted, and what rustc
// loads, given another build's by `--extern`, is checked by its crate hash.
test("a library's manifest beside the metadata of another build is refused", () => {
  const dir = fixture("crates-mixed");
  const src = sources(dir);
  const app = compile(dir, src);
  const other = join(dir, "other");
  const again = command(dir, src, "models", ["validation"]).map((arg) =>
    arg.startsWith("--emit=metadata=") ? `--emit=metadata=${join(other, "libmodels.rmeta")}` : arg.startsWith(js(dir, "models")) ? arg.replace(js(dir, "models"), other) : arg);
  run([...again, "-Cmetadata=another-build"]);
  const previous = readFileSync(app, "utf8");
  const frontend = command(dir, src, "frontend", ["models", "validation"]).map((arg) =>
    arg === `models=${join(js(dir, "models"), "libmodels.rmeta")}` ? `models=${join(other, "libmodels.rmeta")}` : arg);
  expect(() => run(frontend)).toThrow("isn't of the build its JS was made from");
  expect(readFileSync(app, "utf8")).toBe(previous);
}, 300_000);

// Whose traits get dictionaries, and whose types drop, is decided by which
// crates are rust-js's (ADR 0100): a consumer must be told of every library
// its libraries were compiled against, or it would decide otherwise.
test("a consumer given a library but not the libraries it uses is refused", () => {
  const dir = fixture("crates-partial");
  const src = sources(dir);
  compile(dir, src);
  const full = command(dir, src, "frontend", ["models", "validation"]);
  const partial = full.filter((arg, i) => !(arg.endsWith(join("validation", "lib.manifest.json")) && full[i - 1] === "--dependency")
    && !(arg === "--dependency" && full[i + 1]?.endsWith(join("validation", "lib.manifest.json"))));
  expect(partial.length).toBe(full.length - 2);
  expect(() => run(partial)).toThrow("`models` was compiled against `validation`, a rust-js library");
}, 300_000);

// The same crates as a Cargo workspace (ADR 0101), built by Cargo with
// rust-js as its workspace wrapper: each library of the workspace compiled by
// rust-js, dependencies first, and serde, a crate of crates.io, by rustc, as
// Cargo would. `check`, a native binary calling `frontend::main`, is the oracle.
const pin = readFileSync(join(root, "rust-toolchain.toml"), "utf8").match(/channel = "([^"]+)"/)![1];
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

const cargo = (manifest: string, ...args: string[]) => ["cargo", `+${pin}`, ...args, "--offline", "--quiet", "--manifest-path", manifest];
/** `packageName` checked for rust-js's target, with rust-js as Cargo's workspace wrapper. */
const check = (manifestPath: string, options: { packageName?: string, features?: string[], react?: string } = {}) =>
  checkCargo({ manifestPath, toolchain: pin, compiler, offline: true, packageName: "frontend", ...options });

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
  expect(recorded[0].split("\n")[0]).toEndWith(` ${compiler.replaceAll(" ", "\\ ")}`);
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
  writeFileSync(join(app, "lib.rs"), "#![allow(non_snake_case)]\n\npub fn value() -> u32 {\n    shared::seven()\n}\n\npub fn View() -> react::Element {\n    jsx! { <b /> }\n}\n");
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

// The bindings as Cargo dependencies: `react` and `web` are crates rustc
// checks, as `react/build.sh` does, with the `cfg`s of a React release
// (ADR 0043) from its build script.
/** A workspace of one library, `ui`, of `source`, using the react crate at `react`. */
function withReact(dir: string, source: string, react = join(root, "react")): string {
  writeFileSync(join(dir, "Cargo.toml"), '[workspace]\nmembers = ["ui"]\nresolver = "2"\n');
  mkdirSync(join(dir, "ui", "src"), { recursive: true });
  writeFileSync(join(dir, "ui", "Cargo.toml"),
    `[package]\nname = "ui"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nreact = { package = "rust-js-react", path = ${JSON.stringify(react)} }\n`);
  writeFileSync(join(dir, "ui", "src", "lib.rs"), source);
  return join(dir, "Cargo.toml");
}
const counter = `#![allow(non_snake_case)]
use react::{Element, use_state};

pub fn Counter() -> Element {
    let (count, set_count) = use_state(0);
    jsx! {
        <button onClick={move |_| set_count.update(|count| count + 1)}>{count}</button>
    }
}
`;
// What only React 19.2 has: gated out of 18.2's API.
const usesUseEffectEvent = `#![allow(non_snake_case)]
use react::{Element, use_effect_event, use_state};

pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    let log = use_effect_event(move || set_count.set(*count));
    jsx! {
        <button onClick={move |_| log()}>{count}</button>
    }
}
`;
/** The JS of a module, without the lines naming where it's from. */
const body = (js: string) => readFileSync(js, "utf8").split("\n").filter((line) => !line.startsWith("// Generated by rust-js") && !line.startsWith("//# sourceMappingURL")).join("\n");

test("a component of a Cargo workspace using the react crate is the JS it is outside Cargo", async () => {
  buildReact();
  const dir = fixture("cargo-react");
  const manifest = withReact(dir, counter);
  const { js } = await check(manifest, { packageName: "ui" });
  const direct = join(dir, "direct");
  run([compiler, join(dir, "ui", "src", "lib.rs"), "-o", join(direct, "lib.js"), "--library", "--manifest", join(direct, "lib.manifest.json"),
    "--", "--crate-name", "ui", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target]);
  expect(js).toEndWith("lib.jsx");
  expect(body(js)).toContain('import { useState } from "react";');
  expect(body(js)).toBe(body(join(direct, "lib.jsx")));
}, 600_000);

// Found in review: the bindings as the tooling installs them, under the
// app's `node_modules`, are inside its workspace, whose members they are.
test("the bindings installed inside a Cargo workspace are rustc's to check, as outside it", async () => {
  const dir = fixture("cargo-react-inside");
  const resources = join(dir, "node_modules", "rust-js-resources");
  for (const file of bindingInputs.react) cpSync(join(root, file), join(resources, file));
  const manifest = withReact(dir, counter, join(resources, "react"));
  const { js } = await check(manifest, { packageName: "ui" });
  expect(body(js)).toContain('import { useState } from "react";');
  // A check of the whole workspace too, the bindings with it.
  run(cargo(manifest, "check", "--target", "wasm32-unknown-unknown"), 600_000, { RUSTC_WORKSPACE_WRAPPER: compiler });
}, 600_000);

test("a Cargo build is for the React release it's asked for, and follows a change of it", async () => {
  const dir = fixture("cargo-react-versions");
  const manifest = withReact(dir, usesUseEffectEvent);
  const attempt = (react: string) => check(manifest, { packageName: "ui", react });
  // The build script's `cfg`s are `react/build.sh`'s, from `cfg.js`.
  const build = join(dir, "target", "wasm32-unknown-unknown", "debug", "build");
  const flags = () => {
    const runs = readdirSync(build).filter((d) => d.startsWith("rust-js-react-") && existsSync(join(build, d, "output")));
    expect(runs.length).toBe(1);
    return readFileSync(join(build, runs[0], "output"), "utf8").split("\n").filter((line) => line.startsWith("cargo::rustc-"))
      .map((line) => line.replace("cargo::rustc-cfg=", "--cfg=").replace("cargo::rustc-check-cfg=", "--check-cfg="));
  };
  await expect(attempt("18.2.0")).rejects.toThrow("unresolved import `react::use_effect_event`");
  expect(flags()).toEqual(cfgFlags("18.2.0").flags);
  await attempt("19.3.0");
  expect(flags()).toEqual(cfgFlags("19.3.0").flags);
  await expect(attempt("18.2.0")).rejects.toThrow('#[cfg(react = "19.2")]');
}, 600_000);

// Two crates at a time (`test/crates/pairs/<case>/`): a library, `dep`, and an
// `app` using it, for each thing crossing between them that a consumer must
// be told, or must assume. Found in review: each had printed another answer.
const pairs = join(root, "test", "crates", "pairs");
for (const name of readdirSync(pairs).sort()) {
  test(`two crates: ${name}`, () => {
    const dir = fixture(`pair-${name}`);
    const native = join(dir, "native"), out = join(dir, "js");
    mkdirSync(native, { recursive: true });
    const rustc = ["rustc", "--edition=2024", "-Coverflow-checks=off"];
    run([...rustc, "--crate-type=rlib", "--crate-name", "dep", join(pairs, name, "dep.rs"), "-o", join(native, "libdep.rlib")]);
    run([...rustc, "--crate-type=rlib", "--crate-name", "app", join(pairs, name, "app.rs"), "-o", join(native, "libapp.rlib"), "--extern", `dep=${join(native, "libdep.rlib")}`]);
    writeFileSync(join(native, "main.rs"), "fn main() {\n    app::main();\n}\n");
    run([...rustc, join(native, "main.rs"), "-o", join(native, "program"), "--extern", `app=${join(native, "libapp.rlib")}`, "-L", `dependency=${native}`]);
    const lib = join(out, "dep");
    run([compiler, join(pairs, name, "dep.rs"), "-o", join(lib, "lib.js"), "--library", "--manifest", join(lib, "lib.manifest.json"),
      "--", "--crate-name", "dep", `--emit=metadata=${join(lib, "libdep.rmeta")}`]);
    const app = join(out, "app", "lib.js");
    run([compiler, join(pairs, name, "app.rs"), "-o", app, "--dependency", join(lib, "lib.manifest.json"), "--", "--crate-name", "app", "--extern", `dep=${join(lib, "libdep.rmeta")}`]);
    expect(printed(app)).toBe(run([join(native, "program")]));
  }, 300_000);
}

// A library's JS and its metadata are one build's (ADR 0100): rustc writes
// the metadata where it's staged, and it's published with the JS, from one
// plan, or neither is. Found in review, each: JS published beside metadata
// that couldn't be written; metadata replaced beside JS that couldn't be; and
// metadata written over a source of the crate.
describe("a library's metadata is published with its JS, or neither is", () => {
  const library = (dir: string) => {
    const source = join(dir, "lib.rs"), output = join(dir, "js", "lib.js");
    writeFileSync(source, "pub mod data;\npub fn one() -> u32 {\n    data::N\n}\n");
    writeFileSync(join(dir, "data.rs"), "pub const N: u32 = 1;\n");
    const build = (metadata: string) =>
      run([compiler, source, "-o", output, "--library", "--manifest", join(dir, "js", "lib.manifest.json"), "--", "--crate-name", "dep", `--emit=metadata=${metadata}`]);
    return { source, output, build, edit: () => writeFileSync(join(dir, "data.rs"), "pub const N: u32 = 2;\n") };
  };

  test("metadata that can't be published publishes nothing", () => {
    const dir = fixture("metadata-blocked");
    const { output, build, edit } = library(dir);
    build(join(dir, "js", "libdep.rmeta"));
    const [js, meta] = [readFileSync(output, "utf8"), readFileSync(join(dir, "js", "libdep.rmeta"))];
    edit();
    writeFileSync(join(dir, "blocker"), "a file, not a directory\n");
    expect(() => build(join(dir, "blocker", "libdep.rmeta"))).toThrow();
    expect(readFileSync(output, "utf8")).toBe(js);
    expect(readFileSync(join(dir, "js", "libdep.rmeta")).equals(meta)).toBe(true);
    expect(readdirSync(join(dir, "js")).filter((f) => f.startsWith(".rust-js"))).toEqual([]);
  }, 300_000);

  test("JS that can't be published leaves the metadata of its build too", () => {
    const dir = fixture("metadata-readonly-js");
    const { output, build, edit } = library(dir);
    mkdirSync(join(dir, "meta"));
    const metadata = join(dir, "meta", "libdep.rmeta");
    build(metadata);
    const [js, meta] = [readFileSync(output, "utf8"), readFileSync(metadata)];
    edit();
    chmodSync(join(dir, "js"), 0o555);
    try {
      expect(() => build(metadata)).toThrow();
    } finally {
      chmodSync(join(dir, "js"), 0o755);
    }
    expect(readFileSync(output, "utf8")).toBe(js);
    expect(readFileSync(metadata).equals(meta)).toBe(true);
  }, 300_000);

  // rustc's other outputs would be written past rust-js's checks. Found in
  // review: `mir=` beside `metadata=` wrote over a module's source.
  test("rustc's other outputs are refused, however they're spelled", () => {
    const dir = fixture("metadata-and-mir");
    const { source, output } = library(dir);
    const data = join(dir, "data.rs");
    const before = readFileSync(data, "utf8");
    for (const emit of [[`--emit=metadata=${join(dir, "libdep.rmeta")},mir=${data}`], ["--emit", `mir=${data}`]]) {
      expect(() => run([compiler, source, "-o", output, "--library", "--manifest", join(dir, "m.json"), "--", "--crate-name", "dep", ...emit]))
        .toThrow("isn't something rust-js writes");
      expect(readFileSync(data, "utf8")).toBe(before);
    }
  }, 300_000);

  test("metadata asked for where a source of the crate is, is refused", () => {
    const dir = fixture("metadata-over-source");
    const { build } = library(dir);
    const data = join(dir, "data.rs");
    const before = readFileSync(data, "utf8");
    expect(() => build(data)).toThrow("output collision");
    expect(readFileSync(data, "utf8")).toBe(before);
  }, 300_000);
});
