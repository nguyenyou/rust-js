// Differential test: native Rust vs. the JS that rust-js generates.
//
//   examples/fib.rs ──rustc────► native ──► expected results ─┐
//                  └─rust-js───► fib.js ──► actual results ───┴─► must be equal

import { beforeAll, expect, test } from "bun:test";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const target = join(root, "target");

function run(cmd: string[]): string {
  const p = Bun.spawnSync(cmd, { cwd: root, stderr: "pipe" });
  if (p.exitCode !== 0) {
    throw new Error(`${cmd.join(" ")} failed:\n${p.stderr.toString()}`);
  }
  return p.stdout.toString();
}

type Case = { fn: string; args: number[]; value?: number; panic?: string };
let cases: Case[] = [];
let fib: Record<string, (...args: any[]) => number>;
// The multi-file crate: its root, and two of its other modules.
let modules: Record<string, Record<string, (...args: any[]) => number>>;

beforeAll(async () => {
  run(["cargo", "build", "--quiet"]);
  run([join(target, "debug", "rust-js"), "examples/fib.rs", "-o", join(target, "fib.js")]);
  // Same semantics rust-js targets: the release profile, where arithmetic wraps.
  run(["rustc", "--edition=2024", "-Coverflow-checks=off", "--crate-type=lib", "--crate-name=modules",
    "examples/modules/lib.rs", "-o", join(target, "libmodules.rlib")]);
  run(["rustc", "--edition=2024", "-Coverflow-checks=off", "--extern", `modules=${join(target, "libmodules.rlib")}`,
    "test/native.rs", "-o", join(target, "native")]);
  cases = run([join(target, "native")]).trim().split("\n").map((line) => JSON.parse(line));
  fib = await import(join(target, "fib.js"));
  run([join(target, "debug", "rust-js"), "examples/modules/lib.rs", "-o", join(target, "modules", "lib.js")]);
  modules = {
    lib: await import(join(target, "modules", "lib.js")),
    stats: await import(join(target, "modules", "stats.js")),
    util: await import(join(target, "modules", "util.js")),
  };
}, 600_000);

// `nth` takes an enum; in JS a fieldless variant is its name as a string.
function call(c: Case): number {
  switch (c.fn) {
    case "nth_asc":
      return fib.nth("Ascending", ...c.args);
    case "nth_desc":
      return fib.nth("Descending", ...c.args);
    default: {
      // "modules.summary" is the crate root's; "modules.stats.mean" is stats.js's.
      const path = c.fn.split(".");
      if (path[0] === "modules") {
        const [file, name] = path.length === 2 ? ["lib", path[1]] : [path[1], path[2]];
        return modules[file][name](...c.args);
      }
      return fib[c.fn](...c.args);
    }
  }
}

test("generated JS matches native Rust on every case", () => {
  expect(cases.length).toBeGreaterThan(100);
  for (const c of cases) {
    const label = `${c.fn}(${c.args.join(", ")})`;
    if (c.panic !== undefined) {
      expect(() => call(c), label).toThrow(c.panic);
    } else {
      expect([label, call(c)]).toEqual([label, c.value!]);
    }
  }
});

// Source map: generated JS positions must point at the Rust that produced them.
test("source map points from fib.js back into fib.rs", async () => {
  const { decodeMappings, lookup } = await import("./sourcemap.ts");
  const js = (await Bun.file(join(target, "fib.js")).text()).split("\n");
  const map = await Bun.file(join(target, "fib.js.map")).json();
  const rustSource = await Bun.file(join(root, "examples/fib.rs")).text();
  const rs = rustSource.split("\n");
  const segments = decodeMappings(map.mappings);

  // The map sits in target/, so it names the source relative to there,
  // and embeds the Rust source so a debugger can show it.
  expect(map.sources).toEqual(["../examples/fib.rs"]);
  expect(map.sourcesContent).toEqual([rustSource]);

  // Nothing in the header or runtime helpers maps to Rust.
  const firstFunction = js.findIndex((l) => l.startsWith("export function"));
  expect(Math.min(...segments.map((s) => s.jsLine))).toBe(firstFunction);

  // Every mapping lands inside the Rust file.
  for (const s of segments) {
    expect(s.srcLine).toBeLessThan(rs.length);
    expect(s.srcCol).toBeLessThanOrEqual(rs[s.srcLine].length);
  }

  // JS snippet  →  the Rust text its position maps to.
  const probes: [string, string][] = [
    ["export function fib(", "pub fn fib("],
    ["fib(n - 1 >>> 0)", "fib(n - 1)"],
    ["while (i < n)", "while i < n"],
    ["if (i === n)", "if i == n"],
    ["Math.imul(x, 3) - 7 | 0", "x * 3 - 7"],
    ["$div(a, b, -2147483648) | 0", "a / b"],
    ['order === "Ascending"', "Order::Ascending"],
    ["fib_iter(20 - n >>> 0)", "fib_iter(20 - n)"],
  ];
  for (const [jsText, rustText] of probes) {
    const line = js.findIndex((l, i) => i >= firstFunction && l.includes(jsText));
    const col = js[line].indexOf(jsText);
    const hit = lookup(segments, line, col);
    const mapped = hit ? rs[hit.srcLine].slice(hit.srcCol) : "(no mapping)";
    expect([jsText, mapped.startsWith(rustText) ? rustText : mapped]).toEqual([jsText, rustText]);
  }
});

// ADR 0019: one JS file per module, with generated imports and exports.
test("a crate split across files becomes one JS file per module", async () => {
  const out = join(target, "modules");
  const files = [...new Bun.Glob("**/*.js").scanSync(out)].sort();
  // `geometry` only holds other modules, so it gets no file.
  expect(files).toEqual(["geometry/area.js", "geometry/util.js", "lib.js", "stats.js", "util.js"]);

  // Exported: \`pub\` functions, plus private ones another file calls
  // (\`clamp\`, called from child modules). Private and local: not exported.
  expect(Object.keys(modules.lib).sort()).toEqual(["clamp", "doubled_mean", "mixed", "shadowed", "summary"]);
  expect(Object.keys(modules.stats)).toEqual(["mean"]);

  const area = await Bun.file(join(out, "geometry/area.js")).text();
  // Specifiers are relative, and aliases are unique within the file.
  expect(area).toContain('import * as lib from "../lib.js";');
  expect(area).toContain('import * as util from "./util.js";');
  expect(area).toContain('import * as util$1 from "../util.js";');
  // A local named like an alias is renamed rather than shadowing it.
  expect(area).toContain("const util$2 = x + 1 >>> 0;");
  expect(area).toContain("return util$1.double(util$2);");
  // lib ↔ stats import each other: a cycle, which Rust and ES modules allow.
  expect(await Bun.file(join(out, "stats.js")).text()).toContain('import * as lib from "./lib.js";');

  // Each file's source map points into the .rs file its module lives in.
  const sources = async (f: string) => (await Bun.file(join(out, `${f}.map`)).json()).sources;
  expect(await sources("stats.js")).toEqual(["../../examples/modules/stats.rs"]);
  expect(await sources("geometry/area.js")).toEqual(["../../../examples/modules/geometry/area.rs"]);
  // An inline module lives in its parent's file.
  expect(await sources("util.js")).toEqual(["../../examples/modules/lib.rs"]);
});
