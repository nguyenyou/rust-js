// Differential test: native Rust vs. the JS that rust-js generates.
//
//   examples/fib.rs ──rustc────► native ──► expected results ─┐
//                  └─rust-js───► fib.js ──► actual results ───┴─► must be equal

import { beforeAll, expect, test } from "bun:test";
import { copyFileSync } from "node:fs";
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
let collections: Record<string, (...args: any[]) => unknown>;
// The multi-file crate: its root, and two of its other modules.
let modules: Record<string, Record<string, (...args: any[]) => number>>;
// Imports from JS modules: the root, and a module two directories down.
let imports: Record<string, Record<string, () => unknown>>;
let asyncs: Record<string, (...args: any[]) => any>;

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
  run([join(target, "debug", "rust-js"), "examples/collections.rs", "-o", join(target, "collections.js")]);
  collections = await import(join(target, "collections.js"));
  // The web crate is used from its metadata (ADR 0024).
  run(["web/build.sh", "-o", join(target, "libweb.rmeta")]);
  const withWeb = ["--", "--extern", `web=${join(target, "libweb.rmeta")}`];
  run([join(target, "debug", "rust-js"), "examples/counter.rs", "-o", join(target, "counter.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "test/web_forms.rs", "-o", join(target, "web_forms.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "examples/todo.rs", "-o", join(target, "todo.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "examples/countdown.rs", "-o", join(target, "countdown.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "examples/fetch.rs", "-o", join(target, "fetch.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "test/async.rs", "-o", join(target, "async.js"), ...withWeb]);
  asyncs = await import(join(target, "async.js"));
  // Test mode (ADR 0026): the same programs with their `#[test]`s, and some failing on purpose.
  const tests = (rs: string, name: string, flags: string[] = []) =>
    run([join(target, "debug", "rust-js"), "--test", rs, "-o", join(target, "rust-tests", name, `${name}.js`), ...flags]);
  tests("examples/counter.rs", "counter", withWeb);
  tests("examples/todo.rs", "todo", withWeb);
  tests("examples/countdown.rs", "countdown", withWeb);
  tests("test/asserts.rs", "asserts");
  // For real browsers (ADR 0027): `--cfg browser` turns on tests that need one.
  const forBrowser = (rs: string, name: string, flags: string[] = []) =>
    run([join(target, "debug", "rust-js"), "--test", rs, "-o", join(target, "browser-tests", name, `${name}.js`), ...flags, "--cfg=browser"]);
  forBrowser("examples/counter.rs", "counter", withWeb);
  forBrowser("examples/todo.rs", "todo", withWeb);
  forBrowser("test/asserts.rs", "asserts", ["--"]);
  run([join(target, "debug", "rust-js"), "examples/modules/lib.rs", "-o", join(target, "modules", "lib.js")]);
  modules = {
    lib: await import(join(target, "modules", "lib.js")),
    stats: await import(join(target, "modules", "stats.js")),
    util: await import(join(target, "modules", "util.js")),
  };
  // Imports (ADR 0028): `./greet.js` is relative to the root's JS, so it goes beside it.
  run([join(target, "debug", "rust-js"), "test/imports/lib.rs", "-o", join(target, "imports", "lib.js")]);
  copyFileSync(join(root, "test/imports/greet.js"), join(target, "imports", "greet.js"));
  imports = {
    lib: await import(join(target, "imports", "lib.js")),
    leaf: await import(join(target, "imports", "inner", "leaf.js")),
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
      if (path[0] === "collections") {
        return collections[path[1]](...c.args);
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

// ADR 0028: `#[link_name = "module#path"]` imports from a JS module.
test("extern items from JS modules become import statements", async () => {
  // They run: Node's modules, and a hand-written file.
  expect(imports.lib.paths()).toBe("a/b/c.txt");
  expect(imports.lib.file_name()).toBe("y.txt");
  expect(imports.lib.urls()).toEqual(["https://example.com/a", "https://example.com/b"]);
  expect(imports.lib.greetings()).toEqual(["Hello, world!", "Good day, world.", "!?"]);
  expect(imports.leaf.hello()).toBe("Hello, leaf!");

  const lib = await Bun.file(join(target, "imports", "lib.js")).text();
  // One statement per module and kind, as a person would write them. A
  // default or namespace import is named after its module.
  expect(lib).toContain('import greet, { punctuation } from "./greet.js";\nimport * as greet$1 from "./greet.js";\n');
  expect(lib).toContain('import { join, posix } from "node:path";');
  // Beside the global `URL`, the imported one is renamed.
  expect(lib).toContain('import { URL as URL$1 } from "node:url";');
  expect(lib).toContain('return [new URL$1("https://example.com/a").href, new URL("https://example.com/b").href];');
  // A path goes on from the import, and a local doesn't hide one.
  expect(lib).toContain('return posix.basename("/tmp/x/y.txt");');
  expect(lib).toContain('const join$1 = "c.txt";\n  return join(join("a", "b"), join$1);');
  expect(lib).toContain('greet$1.polite("world")');
  // Two directories down, only what the file uses, from the same file.
  const leaf = await Bun.file(join(target, "imports", "inner", "leaf.js")).text();
  expect(leaf).toMatch(/\nimport greet from "\.\.\/greet\.js";\n\nexport function hello/);
});

// ADR 0029: `async fn` is an `async function`, `.await` is `await`, and a
// future is a JS promise.
test("async code becomes async functions and await", async () => {
  expect(await asyncs.sum(2, 3)).toBe(10);
  expect(await asyncs.countdown(4)).toBe(4);
  expect(await asyncs.swap([1, 2])).toEqual([2, 1]);
  expect(await asyncs.blocks(5)).toBe(26);
  expect(await asyncs.held()).toBe(5);
  // A spawned task runs up to its first `.await` at once, the rest later.
  const log = asyncs.spawned();
  expect(log.value).toEqual([1, 2]);
  await Bun.sleep(20);
  expect(log.value).toEqual([1, 2, 3]);
  // `window::fetch_with_str`, from the web crate, and the response's promises.
  // Bun has `fetch`; the web crate reaches it through `window`.
  const server = Bun.serve({ port: 0, fetch: () => new Response("hello", { status: 201 }) });
  (globalThis as any).window = globalThis;
  try {
    expect(await asyncs.load(server.url.href)).toEqual([201, true, "hello"]);
    // Binary data: `bytes()` and `arrayBuffer()`, five bytes of "hello".
    expect(await asyncs.load_bytes(server.url.href)).toEqual([5, 5, 5]);
  } finally {
    delete (globalThis as any).window;
    server.stop();
  }

  const js = await Bun.file(join(target, "async.js")).text();
  expect(js).toContain("export async function sum(a, b) {\n  return await double(a) + await double(b) >>> 0;\n}");
  // Parameters are the body's variables: no `let x = x`.
  expect(js).toContain("export async function countdown(n) {\n  let steps = 0;");
  expect(js).toContain("export async function swap(param) {\n  const a = param[0];");
  // An `async` block is an async arrow, called; an `async` closure, an async arrow.
  expect(js).toContain("const block = (async () => await double(x) + 1 >>> 0)();");
  expect(js).toContain("const add = async (y) => await setTimeout(0, y) + x >>> 0;");
  // A future in a variable is the promise; `.await` on it is `await`.
  expect(js).toContain("const first = setTimeout(5, 1);");
  expect(js).toContain("return await first + await second >>> 0;");

  const countdown = await Bun.file(join(target, "countdown.js")).text();
  expect(countdown).toContain("return new Promise((resolve) => {\n    setTimeout(resolve, ms);\n  });");
  expect(countdown).toContain("    await sleep(500);\n");
  const fetchJs = await Bun.file(join(target, "fetch.js")).text();
  expect(fetchJs).toContain("  const response = await window.fetch(url);\n  const text = await response.text();\n");
  // `spawn(Box::new(load(..)))`: the call is the promise.
  expect(fetchJs).toContain('    load("data:text/plain,Hello from a fetch!", output);\n');
  // `spawn(Box::new(async move { .. }))` is the promise, unawaited.
  expect(countdown).toContain("    (async () => {\n      await count_down(output, 3);\n      running$1.value = false;\n    })();");
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

// ADR 0026: `#[test]` functions, in Rust, compiled by `rust-js --test` and
// run by `bun test` in happy-dom's DOM.
function rustTests(files: string[]): { exit: number; output: string } {
  const p = Bun.spawnSync(["bun", "test", "--preload", "./test/happydom.ts", ...files.map((f) => `./${f}`)], {
    cwd: root,
    stderr: "pipe",
  });
  return { exit: p.exitCode ?? -1, output: p.stdout.toString() + p.stderr.toString() };
}

test("the example apps' own tests pass, in a DOM", () => {
  const apps = ["counter", "todo", "countdown"].map((app) => `target/rust-tests/${app}/${app}.test.js`);
  const { exit, output } = rustTests(apps);
  expect([exit, output.match(/(\d+) pass/)?.[1], output.match(/(\d+) fail/)?.[1]]).toEqual([0, "8", "0"]);
});

test("a failing test fails the way Rust's would", () => {
  const { exit, output } = rustTests(["target/rust-tests/asserts/asserts.test.js"]);
  expect(exit).toBe(1);
  expect([output.match(/(\d+) pass/)?.[1], output.match(/(\d+) skip/)?.[1], output.match(/(\d+) fail/)?.[1]]).toEqual(["3", "1", "4"]);
  // `assert!` with a message; `assert_eq!` showing both sides, as Rust does.
  expect(output).toContain("error: n was 3");
  expect(output).toContain("error: assertion `left == right` failed\n  left: { x: 1, y: 2 }\n right: { x: 1, y: 3 }");
  // `#[should_panic]`: the wrong message, and no panic at all.
  expect(output).toContain('panic message: "\\"something\\" happened"\n expected substring: "nope"');
  expect(output).toContain("error: test did not panic as expected");
  for (const name of ["fails_an_assert", "fails_an_assert_eq", "panics_with_the_wrong_message", "does_not_panic"]) {
    expect(output).toContain(`(fail) tests::${name}`);
  }
});

// ADR 0027: the same tests in real browsers, Chromium, Firefox and WebKit,
// through Playwright Test and through Vitest's browser mode, both on Bun.
const browserTests = ["target/browser-tests/counter/counter.test.js", "target/browser-tests/todo/todo.test.js"];

function inBrowsers(runner: "playwright" | "vitest", files: string[]): { exit: number; output: string } {
  const command =
    runner === "playwright"
      ? ["bunx", "--bun", "playwright", "test", "-c", "browser/playwright.config.ts", "--reporter=line"]
      : ["bunx", "--bun", "vitest", "run", "-c", "browser/vitest.config.ts"];
  const p = Bun.spawnSync(command, { cwd: root, env: { ...process.env, RUST_JS_TESTS: files.join(" ") }, stderr: "pipe" });
  return { exit: p.exitCode ?? -1, output: p.stdout.toString() + p.stderr.toString() };
}

test("in real browsers, with Playwright Test on Bun", () => {
  const { exit, output } = inBrowsers("playwright", browserTests);
  // 8 tests, the layout one included, on 3 engines.
  expect([exit, output.match(/(\d+) passed/)?.[1]]).toEqual([0, "24"]);
  const failing = inBrowsers("playwright", ["target/browser-tests/asserts/asserts.test.js"]);
  expect([failing.exit, failing.output.match(/(\d+) failed/)?.[1], failing.output.match(/(\d+) skipped/)?.[1]]).toEqual([1, "12", "3"]);
  expect(failing.output).toContain("Error: assertion `left == right` failed\n      left: { x: 1, y: 2 }");
}, 120_000);

test("in real browsers, with Vitest's browser mode", () => {
  const { exit, output } = inBrowsers("vitest", browserTests);
  expect([exit, output.match(/Tests\s+(\d+) passed/)?.[1]]).toEqual([0, "24"]);
  const failing = inBrowsers("vitest", ["target/browser-tests/asserts/asserts.test.js"]);
  expect([failing.exit, failing.output.match(/Tests\s+(\d+) failed/)?.[1]]).toEqual([1, "12"]);
  // Vitest follows the source map back into the Rust.
  expect(failing.output).toContain("fails_an_assert test/asserts.rs:");
}, 120_000);

// The counter's JS reads like the Rust: methods, properties, globals, and one
// shared `{ value }`. No wrappers from the web crate.
test("the counter's JS is plain DOM code", async () => {
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
