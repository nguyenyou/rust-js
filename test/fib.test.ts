// Differential test: native Rust vs. the JS that rsjs generates.
//
//   examples/fib.rs ──rustc──► native ──► expected results ─┐
//                  └──rsjs───► fib.js ──► actual results ───┴─► must be equal

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

beforeAll(async () => {
  run(["cargo", "build", "--quiet"]);
  run([join(target, "debug", "rsjs"), "examples/fib.rs", "-o", join(target, "fib.js")]);
  // Same semantics rsjs targets: the release profile, where arithmetic wraps.
  run(["rustc", "--edition=2024", "-Coverflow-checks=off", "test/native.rs", "-o", join(target, "native")]);
  cases = run([join(target, "native")]).trim().split("\n").map((line) => JSON.parse(line));
  fib = await import(join(target, "fib.js"));
}, 600_000);

// `nth` takes an enum; in JS a fieldless variant is its name as a string.
function call(c: Case): number {
  switch (c.fn) {
    case "nth_asc":
      return fib.nth("Ascending", ...c.args);
    case "nth_desc":
      return fib.nth("Descending", ...c.args);
    default:
      return fib[c.fn](...c.args);
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
