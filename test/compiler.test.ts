// Differential test: native Rust vs. the JS that rust-js generates.
//
//   examples/fib.rs ──rustc────► native ──► expected results ─┐
//                  └─rust-js───► fib.js ──► actual results ───┴─► must be equal

import { beforeAll, expect, test } from "bun:test";
import { copyFileSync } from "node:fs";
import { join } from "node:path";

import { root, target, run, buildCompiler, buildReact, buildWeb } from "./support";

// Values are JSON: numbers, and objects and arrays for structs and tuples.
type Case = { fn: string; args: unknown[]; value?: unknown; panic?: string };
let cases: Case[] = [];
let fib: Record<string, (...args: any[]) => number>;
let structs: Record<string, (...args: any[]) => unknown>;
let closures: Record<string, (...args: any[]) => unknown>;
let collections: Record<string, (...args: any[]) => unknown>;
let options: Record<string, (...args: any[]) => unknown>;
let consts: Record<string, (...args: any[]) => unknown>;
let enums: Record<string, (...args: any[]) => unknown>;
let strings: Record<string, (...args: any[]) => unknown>;
let results: Record<string, (...args: any[]) => unknown>;
let iterators: Record<string, (...args: any[]) => unknown>;
let threadLocals: Record<string, (...args: any[]) => unknown>;
let throws: Record<string, (...args: any[]) => any>;
// The multi-file crate: its root, and two of its other modules.
let modules: Record<string, Record<string, (...args: any[]) => number>>;
// Imports from JS modules: the root, and a module two directories down.
let imports: Record<string, Record<string, () => unknown>>;
let asyncs: Record<string, (...args: any[]) => any>;

beforeAll(async () => {
  buildCompiler();
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
  run([join(target, "debug", "rust-js"), "examples/options.rs", "-o", join(target, "options.js")]);
  options = await import(join(target, "options.js"));
  run([join(target, "debug", "rust-js"), "examples/consts.rs", "-o", join(target, "consts.js")]);
  consts = await import(join(target, "consts.js"));
  run([join(target, "debug", "rust-js"), "examples/enums.rs", "-o", join(target, "enums.js")]);
  enums = await import(join(target, "enums.js"));
  run([join(target, "debug", "rust-js"), "examples/strings.rs", "-o", join(target, "strings.js")]);
  strings = await import(join(target, "strings.js"));
  run([join(target, "debug", "rust-js"), "examples/results.rs", "-o", join(target, "results.js")]);
  results = await import(join(target, "results.js"));
  run([join(target, "debug", "rust-js"), "examples/iterators.rs", "-o", join(target, "iterators.js")]);
  iterators = await import(join(target, "iterators.js"));
  run([join(target, "debug", "rust-js"), "examples/thread_locals.rs", "-o", join(target, "thread_locals.js")]);
  threadLocals = await import(join(target, "thread_locals.js"));
  // The web crate is used from its metadata (ADR 0024).
  buildWeb();
  const withWeb = ["--", "--extern", `web=${join(target, "libweb.rmeta")}`];
  run([join(target, "debug", "rust-js"), "examples/counter.rs", "-o", join(target, "counter.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "test/web_forms.rs", "-o", join(target, "web_forms.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "examples/todo.rs", "-o", join(target, "todo.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "examples/countdown.rs", "-o", join(target, "countdown.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "examples/fetch.rs", "-o", join(target, "fetch.js"), ...withWeb]);
  run([join(target, "debug", "rust-js"), "test/throws.rs", "-o", join(target, "throws.js"), ...withWeb]);
  throws = await import(join(target, "throws.js"));
  // The playground's own Rust (ADRs 0032, 0044), as compile-rust.ts compiles it with
  // rust-js.wasm: with React.
  buildReact();
  run([join(target, "debug", "rust-js"), "wasm/web/rust/lib.rs", "-o", join(target, "playground", "lib.js"),
    ...withWeb, "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target]);
  run([join(target, "debug", "rust-js"), "test/async.rs", "-o", join(target, "async.js"), ...withWeb]);
  asyncs = await import(join(target, "async.js"));
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
      if (path[0] === "thread_locals") {
        return threadLocals[path[1]](...c.args);
      }
      if (path[0] === "iterators") {
        return JSON.parse(JSON.stringify(iterators[path[1]](...c.args), (_, x) => (x === undefined ? null : x)));
      }
      if (path[0] === "results") {
        return JSON.parse(JSON.stringify(results[path[1]](...c.args), (_, x) => (x === undefined ? null : x)));
      }
      if (path[0] === "strings") {
        return strings[path[1]](...c.args);
      }
      if (path[0] === "enums") {
        return enums[path[1]](...c.args);
      }
      if (path[0] === "consts") {
        return JSON.parse(JSON.stringify(consts[path[1]](...c.args), (_, x) => (x === undefined ? null : x)));
      }
      if (path[0] === "options") {
        // `None` is `undefined` in JS, and `null` in the JSON: compare them as one.
        const value = options[path[1]](...c.args);
        return JSON.parse(JSON.stringify(value, (_, x) => (x === undefined ? null : x)));
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

// ADR 0019: one JS file per module, with generated imports and exports.
test("a crate split across files becomes one JS file per module", async () => {
  const out = join(target, "modules");
  const files = [...new Bun.Glob("**/*.js").scanSync(out)].sort();
  // `geometry` only holds other modules, so it gets no file.
  expect(files).toEqual(["geometry/area.js", "geometry/util.js", "lib.js", "stats.js", "util.js"]);

  // Exported: \`pub\` functions, plus private ones another file calls
  // (\`clamp\`, called from child modules). Private and local: not exported.
  expect(Object.keys(modules.lib).sort()).toEqual(["HALVES", "clamp", "doubled_mean", "mixed", "shadowed", "summary"]);
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
  const stats = await Bun.file(join(out, "stats.js")).text();
  expect(stats).toContain('import * as lib from "./lib.js";');
  // A `const` of another module, by its name there.
  expect(stats).toContain("return x / lib.HALVES >>> 0;");

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

  // WebAssembly: a module that imports `env.double` and exports
  // `add(a, b) = double(a + b)`, by hand.
  // prettier-ignore
  const wasm = new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,                          // "\0asm", version 1
    0x01, 0x0c, 0x02, 0x60, 0x01, 0x7f, 0x01, 0x7f, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f, // types: (i32) -> i32, (i32, i32) -> i32
    0x02, 0x0e, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x06, 0x64, 0x6f, 0x75, 0x62, 0x6c, 0x65, 0x00, 0x00, // import env.double
    0x03, 0x02, 0x01, 0x01,                                                  // one function, of type 1
    0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x01,                    // export "add"
    0x0a, 0x0b, 0x01, 0x09, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x10, 0x00, 0x0b, // a + b, then call double
  ]);
  expect(await asyncs.run_wasm(wasm, 2, 3)).toBe(10);
  expect(await asyncs.instantiate_bytes(wasm)).toBe(true);

  const js = await Bun.file(join(target, "async.js")).text();
  // A namespace's functions, an overload, and a Rust struct as the import object.
  expect(js).toContain("  const module = await WebAssembly.compile(bytes);\n  const imports = { env: { double: (x) => Math.imul(x, 2) } };\n  const instance = await WebAssembly.instantiate(module, imports);\n  return instance.exports.add(a, b);");
  // A dictionary result is a struct: its fields are read as they are.
  expect(js).toContain("source.instance.exports.add(1, 2) === 3");
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
  expect(fetchJs).toContain('const URL = "data:text/plain,Hello from a fetch!";');
  expect(fetchJs).toContain("    load(URL, output);\n");
  // `spawn(Box::new(async move { .. }))` is the promise, unawaited.
  expect(countdown).toContain("    (async () => {\n      await count_down(output, 3);\n      running$1.value = false;\n    })();");
});

// ADR 0032: the playground is written in Rust in part, compiled by rust-js.
test("the playground is Rust, compiled to the JS main.ts starts", async () => {
  const js = await Bun.file(join(target, "playground", "lib.js")).text();
  expect(js).toContain('import { ConsoleStdout, Directory, File, OpenFile, PreopenDirectory, WASI } from "@bjorn3/browser_wasi_shim";');
  for (const name of ["start", "load", "stat", "ms", "mb", "compile", "render_tree", "link", "resolve", "run_program", "set_status"]) {
    expect(js).toMatch(new RegExp(`^export (async )?function ${name}\\(`, "m"));
  }
  // The downloads all start before any is awaited.
  // (`start` is also the page's entry, so `load`'s own `start` is `start$1`.)
  expect(js).toContain("  const module = load_compiler(start$1);\n  const sysroot = load_sysroot(start$1);");
  expect(js).toContain('  const module = await WebAssembly.compileStreaming(window.fetch("./rust-js.wasm"));');
  // A `format!` value with effects is computed first, once.
  expect(js).toContain('  const arg = t.toFixed(0);\n  return arg + " ms";');
  expect(js).toContain('  const response = await window.fetch("./sysroot/" + name);');
  // A trapped compile is an `Err` (ADR 0035), and `instanceof` a binding.
  expect(js).toContain("  const started = $try(() => wasi.start(instance));");
  expect(js).toContain('  const ok = started.TAG === "Ok" && started._0 === 0;');
  expect(js).toContain("    if (item[1] instanceof Directory) {");
  // The file tree: sorted with a comparator, a copy of the tree's entries.
  expect(js).toContain("  let entries = tree.slice();\n  entries.sort((a, b) => {");
  // Linking: a `RegExp`, and `replace` with a closure, for every kind of export.
  expect(js).toContain('  const exports = new RegExp("^export (async function|function|const) (\\\\w+)", "gm");');
  expect(js).toContain("    const body$1 = body.replace(exports, (_, declared, name) => {");
  // CodeMirror, through imports (ADR 0028), and its key binding a Rust closure.
  expect(js).toContain('import { EditorView, basicSetup } from "codemirror";');
  expect(js).toContain("keymap.of([{");
  // The Result frame's state is thread-locals (ADR 0037).
  expect(js).toContain('const PROGRAM_RUNS = { value: 0 };\nconst REPORTED = { value: false };\nconst RESULT_FRAME = { value: frame_by_id("result") };');
});

// ADR 0037: `thread_local!` is a variable of its module.
test("thread-locals are module variables", async () => {
  const js = await Bun.file(join(target, "thread_locals.js")).text();
  expect(js).toContain("const COUNT = { value: 0 };\nconst LOG = { value: [] };\nconst START = { value: Math.imul(10, 4) + 2 | 0 };");
  expect(js).toContain("  COUNT.value = COUNT.value + 1 >>> 0;\n  return COUNT.value;");
  expect(js).toContain("  })(LOG.value);");
  // Nothing of std's storage.
  expect(js).not.toContain("__rust_std_internal");
});

// ADR 0036: an iterator is a JS array, and `Ordering` a comparator's number.
test("iterators are array methods, and sorting takes comparators", async () => {
  const js = await Bun.file(join(target, "iterators.js")).text();
  expect(js).toContain("  return $range(0, n).map((i) => Math.imul(i, i) >>> 0);");
  // `|&&x|` is `x`: a reference is the value.
  expect(js).toContain("  return v.filter((x) => x % 2 === 0);");
  expect(js).toContain("  const sum = v.reduce((a, b) => a + b | 0, 0);");
  expect(js).toContain("  const anyNegative = v.some((x) => x < 0);");
  expect(js).toContain("  return v.slice(1).slice(0, 2).toReversed();");
  expect(js).toContain('  return words.map((w) => w.toUpperCase()).join("-");');
  expect(js).toContain('  return Array.from(s).toReversed().join("");');
  // Numbers sort by `a - b`: JS's own `sort()` would compare them as strings.
  expect(js).toContain("  w.sort((a, b) => a - b);");
  expect(js).toContain("  w.sort((a, b) => $cmp(key(a), key(b)));");
  // `then_with` is `||`: `Equal` is 0.
  expect(js).toContain("  w.sort((a, b) => $cmp(a.length === 0, b.length === 0) || $cmp(a, b));");
  expect(js).toContain("  if (match === -1) {");
});

// ADR 0035: JS that throws, as a `Result`; and `?`.
test("a throwing JS call is a Result, and ? returns early", async () => {
  expect(throws.sum_json("[1, 2, 3]")).toEqual({ TAG: "Ok", _0: 6 });
  const bad = throws.sum_json("[1, 2,");
  expect(bad.TAG).toBe("Err");
  expect(bad._0).toStartWith("SyntaxError");
  // `?` hands the caught error on as it is.
  expect(throws.first_twice("[4, 5]")).toEqual({ TAG: "Ok", _0: 8 });
  expect(throws.first_twice("{")._0).toBeInstanceOf(SyntaxError);
  // A rejected promise is an `Err` at its `.await`.
  expect(await throws.settled(false)).toBe("7");
  expect(await throws.settled(true)).toBe("rejected: no");

  const js = await Bun.file(join(target, "throws.js")).text();
  expect(js).toContain("  const match = $try(() => JSON.parse(json));");
  expect(js).toContain('  const result = $try(() => JSON.parse(json));\n  if (result.TAG === "Err") {\n    return result;\n  }');
  expect(js).toContain('await $settle(Promise.reject("no"))');
  const results = await Bun.file(join(target, "results.js")).text();
  // On an option, the value keeps the variable's name.
  expect(results).toContain("  const a = half(n);\n  if (a == null) {\n    return undefined;\n  }");
  expect(results).toContain('    r.TAG === "Ok" ? r._0 : 99,');
});

// ADR 0034: strings are JS strings, and their methods JS's.
test("string methods are JS's, and format! is concatenation", async () => {
  const js = await Bun.file(join(target, "strings.js")).text();
  expect(js).toContain('  return name + ": " + String(n) + " item" + (n === 1 ? "" : "s");');
  expect(js).toContain('    s.startsWith("ab"),\n    s.endsWith("c"),\n    s.includes("b/"),\n    s.includes("/")');
  expect(js).toContain('  return s.replaceAll("/", " / ").replaceAll("a", "A");');
  expect(js).toContain('  return $stripSuffix(file, ".rs") ?? file;');
  expect(js).toContain('  for (const part of path.split("/")) {');
  expect(js).toContain('  const last = path.split("/").at(-1) ?? "";');
  expect(js).toContain('  return pieces.join(" > ");');
  // A string that grows gets a new one each time: JS strings don't change.
  expect(js).toContain('    s = s + String(i);\n    s = s + ",";');
  // A `char` is a one-character string.
  expect(js).toContain('  const c = windows ? "\\\\" : "/";');
  // A string literal pattern is `===` on the JS string, without `!= null` in `Some`.
  expect(js).toContain('  } else if (s === "abc" || s === "stats.rs") {');
  expect(js).toContain('  if (top === "ab") {');
});

// ADR 0033: enums with fields, in ReScript's shapes.
test("enums with fields are tagged objects, as in ReScript", async () => {
  const js = await Bun.file(join(target, "enums.js")).text();
  // A variant without fields is its name; one with fields, `{ TAG, _0 }` or named fields.
  expect(js).toContain('  return "Empty";');
  expect(js).toContain('  return {\n    TAG: "Circle",\n    _0: r\n  };');
  expect(js).toContain('  return {\n    TAG: "Rect",\n    w,\n    h\n  };');
  // Matching tests the name, or the `TAG`, then the fields, in place.
  expect(js).toContain('  if (s === "Empty") {\n    return 0;\n  } else if (s.TAG === "Circle") {');
  expect(js).toContain('  } else if (s.TAG === "Rect" && s.w === 0 || s === "Empty") {');
  // Through a reference, with no copy: the reference is the value.
  expect(js).toContain("  if (t.TAG === \"Leaf\") {\n    return t._0;\n  } else {\n    return sum(t._0) + sum(t._1) | 0;");
  // `Result` is ReScript's `result`.
  expect(js).toContain('  if (match.TAG === "Ok") {\n    return match._0;');
});

// ADR 0031: a `const` is the value rustc computed, under its own name.
test("constants are the values rustc computed, by name", async () => {
  const js = await Bun.file(join(target, "consts.js")).text();
  expect(js).toContain("export const SIZE = 4096;\nconst GREETING = \"hello\";\nconst RATIO = .25;\nconst ON = true;");
  expect(js).toContain('const NOTHING = undefined;\nconst LEVEL = "High";');
  // A `const` inside a function goes beside it.
  expect(js).toContain("const STEP = 3;");
  // Each use is a value of its own: copied where it's changed.
  expect(js).toContain("  let p = { ...ORIGIN };\n");
  expect(js).toContain("  return [{ ...p }, { ...ORIGIN }];");
  // A known divisor needs no check for zero.
  expect(js).toContain("  return SIZE / 1024 >>> 0;");
  // std's are written in place.
  expect(js).toContain("  return [4294967295, -2147483648];");
  expect(js).toContain("  for (const p of PRIMES) {");
});

// ADR 0030: `Some(x)` is `x`, `None` is `undefined`, and `null` counts as `None`.
test("options are the value or undefined", async () => {
  const js = await Bun.file(join(target, "options.js")).text();
  expect(js).toContain("    return n / 2 | 0;\n  } else {\n    return undefined;");
  // `Some(0)` needs no `!= null`; `Some(n)` does.
  expect(js).toContain("  if (o === 0) {\n    return 100;\n  } else if (o != null && o < 0) {");
  // `if let Some(h) = ..` keeps the value in a `const h`.
  expect(js).toContain("  const h = half(n);\n  if (h != null) {\n    return h;");
  expect(js).toContain("    h != null,\n    h == null,\n    h ?? -1");
  // `unwrap_or`'s argument runs even when it isn't needed, as in Rust.
  expect(js).toContain("  const option = half(n);\n  const fallback = bump();\n  const v = option ?? fallback;");
  expect(js).toContain('  return $unwrap(half(n), "an even number");');
  // `==` on options is `==`: `null` from JS equals `undefined` from Rust.
  expect(js).toContain("  return a == b;");
  expect(options.same(null, undefined)).toBe(true);
  expect(options.describe(null)).toBe(0);
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
  // The tuple `(u32, u32)` is never changed in place, so it is taken apart as it is.
  expect(js).toContain("const [q, r] = divmod(a, b);");
  // Fields are listed in declaration order, but the calls run in the order written.
  expect(js).toMatch(/const y = \$div\(100, a, -2147483648\) \| 0;\s+const x = \$rem/);
  // A tuple parameter is taken apart where it is, and `match (a, b)` tests its
  // parts directly, without building an array.
  expect(js).toContain("export function classify([a, b]) {\n  if (a === 0 && b === 0) {");
});

// The counter's JS reads like the Rust: methods, properties, globals, and one
// shared `{ value }`. No wrappers from the web crate.
test("the counter's JS is plain DOM code", async () => {
  const js = await Bun.file(join(target, "counter.js")).text();
  expect(js).toContain('const b = document.createElement("button");');
  expect(js).toContain("b.textContent = label;");
  expect(js).toContain("const count = { value: 0 };");
  expect(js).toContain('b.addEventListener("click", () => {');
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
  // A result that may be `null` is an `Option` (ADR 0030), unwrapped here.
  expect(js).toContain('const app = $unwrap(document.getElementById("app"), "the page has an #app");');
  expect(js).toContain("return $unwrap(app.textContent) + input.value;");
  // A union member other than the first gets its own Rust function, same JS.
  expect(js).toContain("app.append(input);");
  expect(js).toContain('app.append("!");');
  // Constructors, and a global used as an `EventTarget` through `Deref`.
  expect(js).toContain('const ping = new Event("ping");');
  // A closure returning \`()\` is a block body: JS gets no return value Rust didn't have.
  expect(js).toContain('app.addEventListener("ping", (e) => {\n    e.preventDefault();\n  });');
  expect(js).toContain("window.dispatchEvent(ping);");
  // Optional arguments: `encode_with_input`, and a union member by type.
  expect(js).toContain('const bytes = new TextEncoder().encode(text);');
  expect(js).toContain('const back = new TextDecoder("utf-8").decode(bytes);');
  const { round_trip } = await import(join(target, "web_forms.js"));
  // "é" is two bytes in UTF-8.
  expect(round_trip("héllo")).toEqual([6, "héllo"]);
});

