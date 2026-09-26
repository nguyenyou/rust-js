import { beforeAll, expect, test } from "bun:test";
import { join } from "node:path";
import { root, target, run, buildCompiler } from "./support";

beforeAll(() => {
  buildCompiler();
  run([join(target, "debug", "rust-js"), "examples/fib.rs", "-o", join(target, "fib.js")]);
}, 600_000);

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
    ["fib((n - 1) >>> 0)", "fib(n - 1)"],
    ["while (i < n)", "while i < n"],
    ["if (i === n)", "if i == n"],
    ["(Math.imul(x, 3) - 7) | 0", "x * 3 - 7"],
    ["$div(a, b, -2147483648) | 0", "a / b"],
    ['order === "Ascending"', "Order::Ascending"],
    ["fib_iter((20 - n) >>> 0)", "fib_iter(20 - n)"],
  ];
  for (const [jsText, rustText] of probes) {
    const line = js.findIndex((l, i) => i >= firstFunction && l.includes(jsText));
    const col = js[line].indexOf(jsText);
    const hit = lookup(segments, line, col);
    const mapped = hit ? rs[hit.srcLine].slice(hit.srcCol) : "(no mapping)";
    expect([jsText, mapped.startsWith(rustText) ? rustText : mapped]).toEqual([jsText, rustText]);
  }
});


// JSX's output extension must participate in collision checks before writing.
test("root and child output collisions leave no partial artifacts", async () => {
  const { fixture, compiler } = await import("./support");
  const { existsSync, writeFileSync } = await import("node:fs");
  for (const jsx of [false, true]) {
    const dir = fixture("collision");
    const input = join(dir, "lib.rs"), output = join(dir, "same.js");
    const body = jsx ? '#[rust_js::link_name = "<div>"] fn el() -> i32 { unreachable!() } pub fn f() -> i32 { el() }' : 'pub fn f() -> i32 { 1 }';
    writeFileSync(input, `${body}\npub mod same { ${body} }`);
    const p = Bun.spawnSync([compiler, input, "-o", output]);
    expect(p.exitCode).not.toBe(0);
    expect(p.stderr.toString()).toContain("collision");
    expect(existsSync(output)).toBe(false);
    expect(existsSync(join(dir, "same.jsx"))).toBe(false);
  }
});

test("manifest owns artifacts and records even modules that emit no code", async () => {
  const { fixture, compiler } = await import("./support");
  const { existsSync, writeFileSync } = await import("node:fs");
  const dir = fixture("manifest");
  const input = join(dir, "lib.rs"), output = join(dir, "lib.js"), manifest = join(dir, "manifest.json");
  writeFileSync(join(dir, "types.rs"), "pub struct Props { pub x: i32 }");
  writeFileSync(input, 'mod types; #[rust_js::link_name = "<div>"] fn el() -> i32 { unreachable!() } pub fn f() -> i32 { el() }');
  const args = [compiler, input, "-o", output, "--manifest", manifest];
  run(args);
  const first = await Bun.file(manifest).json();
  expect(first.sources).toContain(join(dir, "types.rs"));
  expect(first.artifacts.map(a => a.file)).toContain(join(dir, "lib.jsx"));
  writeFileSync(input, "pub fn f() -> i32 { 2 }");
  run(args);
  expect(existsSync(join(dir, "lib.jsx"))).toBe(false);
  expect(existsSync(join(dir, "lib.jsx.map"))).toBe(false);
  expect(existsSync(output)).toBe(true);
  writeFileSync(output, "user edited this file");
  writeFileSync(input, '#[rust_js::link_name = "<div>"] fn el() -> i32 { unreachable!() } pub fn f() -> i32 { el() }');
  run(args);
  expect(await Bun.file(output).text()).toBe("user edited this file");
});

test("mixed JS and JSX modules use their final paths in imports and the manifest", async () => {
  const { fixture, compiler } = await import("./support");
  const dir = fixture("mixed-modules");
  await Bun.write(join(dir, "lib.rs"), `mod view; mod plain; pub fn f() -> i32 { view::render() + plain::value() }`);
  await Bun.write(join(dir, "view.rs"), '#[rust_js::link_name = "<div>"] fn el() -> i32 { unreachable!() } pub fn render() -> i32 { el() }');
  await Bun.write(join(dir, "plain.rs"), 'pub fn value() -> i32 { 1 }');
  const manifest = join(dir, "manifest.json");
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js"), "--manifest", manifest]);
  const code = await Bun.file(join(dir, "lib.js")).text();
  expect(code).toContain('from "./view.jsx"');
  expect(code).toContain('from "./plain.js"');
  const result = await Bun.file(manifest).json();
  expect(result.modules.find(m => m.module.length === 0).imports.sort()).toEqual([join(dir, "plain.js"), join(dir, "view.jsx")]);
  expect(result.modules.find(m => m.module[0] === "view").map).toBe(join(dir, "view.jsx.map"));
});

test("an output map symlink cannot overwrite an input source", async () => {
  const { fixture, compiler } = await import("./support");
  const { symlinkSync } = await import("node:fs");
  const dir = fixture("source-collision");
  const input = join(dir, "lib.rs"), output = join(dir, "out.js");
  const source = "pub fn value() -> i32 { 1 }";
  await Bun.write(input, source);
  symlinkSync(input, output + ".map");
  const p = Bun.spawnSync([compiler, input, "-o", output]);
  expect(p.exitCode).not.toBe(0);
  expect(p.stderr.toString()).toContain("collision");
  expect(await Bun.file(output).exists()).toBe(false);
  expect(await Bun.file(input).text()).toBe(source);
});
