// Build history (ADR 0091): what rust-js writes depends only on what it's
// given. The same input is the same bytes, each time and wherever it's built,
// and a build over an older one leaves what a build from nothing does.
//
//   v1 ──► v2 ──► v3 ──► v4 ──► v1        each warm build over the last
//   ║      ║      ║      ║      ║
//   v1     v2     v3     v4     v1        each a clean build, from nothing: the same tree

import { beforeAll, expect, test } from "bun:test";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";

import { buildCompiler, compiler, fixture, root } from "./support";

beforeAll(buildCompiler);

/** Every file under `dir`, by its path from there, and what's in it, with
 * `project` written `<dir>`: where a build was made isn't what it made,
 * though its manifest says where its sources are, for a build tool to watch. */
function tree(dir: string, project = dir): Record<string, string> {
  const files: Record<string, string> = {};
  const walk = (at: string) => {
    for (const entry of readdirSync(at, { withFileTypes: true })) {
      const path = join(at, entry.name);
      if (entry.isDirectory()) walk(path);
      else files[relative(dir, path)] = readFileSync(path, "utf8").replaceAll(project, "<dir>");
    }
  };
  walk(dir);
  return files;
}

/** rust-js, run in `cwd` as a build tool runs it: a crate root, its JS, and
 * the manifest that says what it wrote. */
function compile(cwd: string, input: string, output: string, manifest?: string) {
  const args = [compiler, input, "-o", output, ...(manifest ? ["--manifest", manifest] : [])];
  const p = Bun.spawnSync(args, { cwd, stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`${args.join(" ")} failed:\n${p.stderr.toString()}`);
}

// Crates of one file and of many, of every kind the examples have.
const examples = ["fib", "structs", "closures", "collections", "options", "numbers", "text", "iterators", "enums", "results", "traits", "consts"];

test("the same input is the same bytes, build after build", () => {
  for (const name of examples) {
    const builds = [1, 2, 3].map(() => {
      const dir = fixture(`history-${name}`);
      compile(root, `examples/${name}.rs`, join(dir, `${name}.js`));
      return tree(dir);
    });
    expect([name, builds[1]]).toEqual([name, builds[0]]);
    expect([name, builds[2]]).toEqual([name, builds[0]]);
  }
});

test("a crate is the same bytes wherever it's built", () => {
  const builds = ["here", join("deeper", "down", "there")].map((place) => {
    const dir = join(fixture("history-place"), place);
    mkdirSync(dir, { recursive: true });
    cpSync(join(root, "examples/modules"), join(dir, "modules"), { recursive: true });
    compile(join(dir, "modules"), "lib.rs", "out/lib.js", "out/manifest.json");
    return tree(join(dir, "modules", "out"), join(dir, "modules"));
  });
  expect(Object.keys(builds[0]).length).toBeGreaterThan(4);
  expect(builds[1]).toEqual(builds[0]);
});

// A crate's sources, version by version: modules added, taken away,
// renamed and made inline, and then as it began.
const versions: Record<string, string>[] = [
  {
    "lib.rs": "mod a;\nmod b;\npub fn f() -> u32 {\n    a::x() + b::y()\n}\n",
    "a.rs": "pub fn x() -> u32 {\n    1\n}\n",
    "b.rs": "pub fn y() -> u32 {\n    super::a::x() + 10\n}\n",
  },
  {
    "lib.rs": "mod a;\nmod c;\npub fn f() -> u32 {\n    a::x() + c::z()\n}\n",
    "a.rs": "pub fn x() -> u32 {\n    2\n}\n",
    "c.rs": "pub fn z() -> u32 {\n    100\n}\n",
  },
  {
    "lib.rs": "mod d;\nmod c;\npub fn f() -> u32 {\n    d::x() + c::z()\n}\n",
    "d.rs": "pub fn x() -> u32 {\n    3\n}\n",
    "c.rs": "pub mod deep;\npub fn z() -> u32 {\n    deep::w()\n}\n",
    "c/deep.rs": "pub fn w() -> u32 {\n    1000\n}\n",
  },
  {
    "lib.rs": "mod util {\n    pub fn x() -> u32 {\n        4\n    }\n}\npub fn f() -> u32 {\n    util::x()\n}\n",
  },
];
const answers = [12, 102, 1003, 4];

/** Makes `dir`'s sources `files`, and only those. */
function write(dir: string, files: Record<string, string>) {
  rmSync(join(dir, "src"), { recursive: true, force: true });
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, "src", path)), { recursive: true });
    writeFileSync(join(dir, "src", path), text);
  }
}

async function answer(output: string): Promise<unknown> {
  const run = Bun.spawnSync([process.execPath, "-e", `import(${JSON.stringify(output)}).then((m) => console.log(m.f()))`]);
  return Number(run.stdout.toString());
}

test("a build over an older one leaves what a build from nothing does", async () => {
  const warm = fixture("history-warm");
  const history = [...versions, versions[0]];
  const first: Record<string, string>[] = [];
  for (const [step, files] of history.entries()) {
    write(warm, files);
    compile(warm, "src/lib.rs", "out/lib.js", "out/manifest.json");
    const clean = fixture("history-clean");
    write(clean, files);
    compile(clean, "src/lib.rs", "out/lib.js", "out/manifest.json");
    const warmTree = tree(join(warm, "out"), warm), cleanTree = tree(join(clean, "out"), clean);
    // Its own directory is `<dir>` in each, so a tree of one is a tree of the other.
    expect([step, warmTree]).toEqual([step, cleanTree]);
    expect([step, await answer(join(warm, "out", "lib.js"))]).toEqual([step, answers[step % versions.length]]);
    first.push(warmTree);
  }
  // As it began: the same bytes as the first build.
  expect(first[history.length - 1]).toEqual(first[0]);
  // A module taken away took its files with it.
  expect(existsSync(join(warm, "out", "b.js"))).toBe(true);
  write(warm, versions[3]);
  compile(warm, "src/lib.rs", "out/lib.js", "out/manifest.json");
  for (const gone of ["a.js", "b.js", "c.js", "d.js", "c/deep.js", "a.js.map"]) {
    expect([gone, existsSync(join(warm, "out", gone))]).toEqual([gone, false]);
  }
});
