// Separate crates (ADR 0100): each compiled once, by rust-js, to JS of its
// own and the metadata rustc reads of it, and a crate using it told what it
// needs by the manifest beside them.
//
//   test/crates/validation ◄── models ◄── frontend
//
// Natively, the same crates are rlibs, and `main` a program's. The JS
// must print what the native program does.

import { beforeAll, describe, expect, test } from "bun:test";
import { chmodSync, cpSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { node } from "./programs";
import { buildCompiler, buildSerde, compiler, fixture, root, run } from "./support";

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
