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

// Values are JSON: numbers, and objects and arrays for structs and tuples.
type Case = { fn: string; args: unknown[]; value?: unknown; panic?: string };
let cases: Case[] = [];
let fib: Record<string, (...args: any[]) => number>;
let structs: Record<string, (...args: any[]) => unknown>;
let closures: Record<string, (...args: any[]) => unknown>;
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
  run([join(target, "debug", "rust-js"), "examples/structs.rs", "-o", join(target, "structs.js")]);
  structs = await import(join(target, "structs.js"));
  run([join(target, "debug", "rust-js"), "examples/closures.rs", "-o", join(target, "closures.js")]);
  closures = await import(join(target, "closures.js"));
  // The web crate is used from its metadata (ADR 0024).
  run(["web/build.sh", "-o", join(target, "libweb.rmeta")]);
  const withWeb = ["--", "--extern", `web=${join(target, "libweb.rmeta")}`];
  run([join(target, "debug", "rust-js"), "examples/counter.rs", "-o", join(target, "counter.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "test/web_forms.rs", "-o", join(target, "web_forms.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "examples/modules/lib.rs", "-o", join(target, "modules", "lib.js")]);
  modules = {
    lib: await import(join(target, "modules", "lib.js")),
    stats: await import(join(target, "modules", "stats.js")),
    util: await import(join(target, "modules", "util.js")),
  };
}, 600_000);

// `nth` takes an enum; in JS a fieldless variant is its name as a string.
function call(c: Case): unknown {
  switch (c.fn) {
    case "nth_asc":
      return fib.nth("Ascending", ...(c.args as number[]));
    case "nth_desc":
      return fib.nth("Descending", ...(c.args as number[]));
    default: {
      // "modules.summary" is the crate root's; "modules.stats.mean" is stats.js's.
      const path = c.fn.split(".");
      if (path[0] === "structs") {
        return structs[path[1]](...c.args);
      }
      if (path[0] === "closures") {
        return closures[path[1]](...c.args);
      }
      if (path[0] === "modules") {
        const [file, name] = path.length === 2 ? ["lib", path[1]] : [path[1], path[2]];
        return modules[file][name](...(c.args as number[]));
      }
      return fib[c.fn](...(c.args as number[]));
    }
  }
}

test("generated JS matches native Rust on every case", () => {
  expect(cases.length).toBeGreaterThan(100);
  for (const c of cases) {
    const label = `${c.fn}(${c.args.map((a) => JSON.stringify(a)).join(", ")})`;
    if (c.panic !== undefined) {
      expect(() => call(c), label).toThrow(c.panic);
    } else {
      expect([label, call(c)]).toEqual([label, c.value]);
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

// ADR 0020: structs are objects, tuples are arrays, and only some reads copy.
test("structs and tuples are plain objects and arrays", async () => {
  const js = await Bun.file(join(target, "structs.js")).text();
  // A JS caller builds the same shapes by hand.
  expect(structs.area({ origin: { x: 0, y: 0 }, size: [3, 4] })).toBe(12);
  expect(structs.classify([5, 5])).toBe(2);

  // `Point` is Copy and changed in place in this crate, so reading one copies it...
  expect(js).toContain("let b = { ...a };");
  expect(js).toContain("origin: { ...a },");
  // ...but `Rect` isn't Copy: assigning it moves it, with no copy.
  expect(js).toContain("let s = r;");
  // Returning a variable hands it over.
  expect(js).toContain("return p;");
  // The tuple `(u32, u32)` is never changed in place, so it's never copied.
  expect(js).toContain("const tmp = divmod(a, b);");
  // Fields are listed in declaration order, but the calls run in the order written.
  expect(js).toMatch(/const y = \$div\(100, a, -2147483648\) \| 0;\s+const x = \$rem/);
  // `match (a, b)` tests the variables directly, without building an array.
  expect(js).toContain("if (param[0] === 0 && param[1] === 0)");
});

// The counter (examples/counter.rs) against a small fake DOM: just the parts
// of it the counter uses, through the web crate (ADR 0024).
class FakeElement {
  children: (FakeElement | string)[] = [];
  listeners: Record<string, ((event: object) => void)[]> = {};
  constructor(readonly tag: string, readonly id = "") {}
  append(child: FakeElement) {
    this.children.push(child);
  }
  set textContent(text: string) {
    this.children = [text];
  }
  get textContent(): string {
    return this.children.map((c) => (typeof c === "string" ? c : c.textContent)).join("");
  }
  addEventListener(event: string, listener: (event: object) => void) {
    (this.listeners[event] ??= []).push(listener);
  }
  click() {
    for (const listener of this.listeners.click ?? []) listener({ type: "click" });
  }
  get text(): string {
    return this.textContent;
  }
}

test("the counter runs against the DOM", async () => {
  const app = new FakeElement("div", "app");
  (globalThis as any).document = {
    getElementById: (id: string) => (id === "app" ? app : null),
    createElement: (tag: string) => new FakeElement(tag),
  };
  try {
    const counter = await import(join(target, "counter.js"));
    counter.main();
    const [minus, output, plus] = app.children as FakeElement[];
    expect([minus.tag, minus.text, output.tag, output.text, plus.text]).toEqual(["button", "-", "output", "0", "+"]);
    plus.click();
    plus.click();
    plus.click();
    minus.click();
    // Both buttons share one count, through the `Rc<Cell<i32>>`.
    expect(output.text).toBe("2");
    minus.click();
    minus.click();
    minus.click();
    expect(output.text).toBe("-1");
  } finally {
    delete (globalThis as any).document;
  }

  // The JS reads like the Rust: methods, properties, globals, and one
  // shared `{ value }`. No wrappers from the web crate.
  const js = await Bun.file(join(target, "counter.js")).text();
  expect(js).toContain('const b = document.createElement("button");');
  expect(js).toContain("b.textContent = label;");
  expect(js).toContain("const count = { value: 0 };");
  expect(js).toContain('b.addEventListener("click", (_) => {');
  expect(js).toContain("count$1.value = count$1.value + by | 0;");
  expect(js).toContain("output.textContent = String(count$1.value);");
  expect(js).toContain("app.append(output);");
});

// Each `#[link_name]` form the web crate uses (ADR 0024), in test/web_forms.rs.
test("the web crate's bindings become plain JS", async () => {
  const js = await Bun.file(join(target, "web_forms.js")).text();
  // A cast is the value itself; a setter, an assignment; a getter, a read.
  expect(js).toContain('const input = document.createElement("input");');
  expect(js).toContain('input.value = "typed";');
  expect(js).toContain("return app.textContent + input.value;");
  // A union member other than the first gets its own Rust function, same JS.
  expect(js).toContain("app.append(input);");
  expect(js).toContain('app.append("!");');
  // Constructors, and a global used as an `EventTarget` through `Deref`.
  expect(js).toContain('const ping = new Event("ping");');
  // A closure returning \`()\` is a block body: JS gets no return value Rust didn't have.
  expect(js).toContain('app.addEventListener("ping", (e) => {\n    e.preventDefault();\n  });');
  expect(js).toContain("window.dispatchEvent(ping);");
});
