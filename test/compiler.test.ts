// Differential test: native Rust vs. the JS that rust-js generates.
//
//   examples/fib.rs ──rustc────► native ──► expected results ─┐
//                  └─rust-js───► fib.js ──► actual results ───┴─► must be equal

import { beforeAll, expect, test } from "bun:test";
import { copyFileSync, rmSync } from "node:fs";
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
let methods: Record<string, (...args: any[]) => unknown>;
let genericOptions: Record<string, (...args: any[]) => unknown>;
let stdTraits: Record<string, (...args: any[]) => unknown>;
let combinators: Record<string, (...args: any[]) => unknown>;
let text: Record<string, (...args: any[]) => unknown>;
let calc: Record<string, (...args: any[]) => unknown>;
let numbers: Record<string, (...args: any[]) => unknown>;
let inventory: Record<string, (...args: any[]) => unknown>;
let queues: Record<string, (...args: any[]) => unknown>;
let report: Record<string, (...args: any[]) => unknown>;
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
  run([join(target, "debug", "rust-js"), "examples/methods.rs", "-o", join(target, "methods.js")]);
  methods = await import(join(target, "methods.js"));
  run([join(target, "debug", "rust-js"), "examples/generic_options.rs", "-o", join(target, "generic_options.js")]);
  genericOptions = await import(join(target, "generic_options.js"));
  run([join(target, "debug", "rust-js"), "examples/std_traits.rs", "-o", join(target, "std_traits.js")]);
  stdTraits = await import(join(target, "std_traits.js"));
  run([join(target, "debug", "rust-js"), "examples/combinators.rs", "-o", join(target, "combinators.js")]);
  combinators = await import(join(target, "combinators.js"));
  run([join(target, "debug", "rust-js"), "examples/text.rs", "-o", join(target, "text.js")]);
  text = await import(join(target, "text.js"));
  run([join(target, "debug", "rust-js"), "examples/calc.rs", "-o", join(target, "calc.js")]);
  calc = await import(join(target, "calc.js"));
  run([join(target, "debug", "rust-js"), "examples/numbers.rs", "-o", join(target, "numbers.js")]);
  numbers = await import(join(target, "numbers.js"));
  run([join(target, "debug", "rust-js"), "examples/inventory.rs", "-o", join(target, "inventory.js")]);
  inventory = await import(join(target, "inventory.js"));
  run([join(target, "debug", "rust-js"), "examples/queues.rs", "-o", join(target, "queues.js")]);
  queues = await import(join(target, "queues.js"));
  run([join(target, "debug", "rust-js"), "examples/report.rs", "-o", join(target, "report.js")]);
  report = await import(join(target, "report.js"));
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
  // Into an empty folder, so a file an older layout wrote can't pass for its output.
  rmSync(join(target, "playground"), { recursive: true, force: true });
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
      if (path[0] === "generic_options") {
        return JSON.parse(JSON.stringify(genericOptions[path[1]](...c.args), (_, x) => (x === undefined ? null : x)));
      }
      if (path[0] === "combinators") {
        return combinators[path[1]](...c.args);
      }
      if (path[0] === "text") {
        return text[path[1]](...c.args);
      }
      if (path[0] === "calc") {
        return calc[path[1]](...c.args);
      }
      if (path[0] === "numbers") {
        return numbers[path[1]](...c.args);
      }
      if (path[0] === "inventory") {
        return inventory[path[1]](...c.args);
      }
      if (path[0] === "queues") {
        return queues[path[1]](...c.args);
      }
      if (path[0] === "report") {
        return report[path[1]](...c.args);
      }
      if (path[0] === "std_traits") {
        return stdTraits[path[1]](...c.args);
      }
      if (path[0] === "methods") {
        return methods[path[1]](...c.args);
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
  expect(area).toContain('import * as util$1 from "./util.js";');
  expect(area).toContain('import * as util$2 from "../util.js";');
  // Imports are named after locals are known, so locals keep their names.
  expect(area).toContain("const util = (x + 1) >>> 0;");
  expect(area).toContain("return util$2.double(util);");
  // lib ↔ stats import each other: a cycle, which Rust and ES modules allow.
  const stats = await Bun.file(join(out, "stats.js")).text();
  expect(stats).toContain('import * as lib from "./lib.js";');
  // A `const` of another module, by its name there.
  expect(stats).toContain("return (x / lib.HALVES) >>> 0;");

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
  expect(js).toContain("export async function sum(a, b) {\n  return ((await double(a)) + (await double(b))) >>> 0;\n}");
  // Parameters are the body's variables: no `let x = x`.
  expect(js).toContain("export async function countdown(n) {\n  let steps = 0;");
  expect(js).toContain("export async function swap(param) {\n  const a = param[0];");
  // An `async` block is an async arrow, called; an `async` closure, an async arrow.
  expect(js).toContain("const block = (async () => ((await double(x)) + 1) >>> 0");
  expect(js).toContain("const add = async (y) => ((await setTimeout(0, y)) + x) >>> 0;");
  // A future in a variable is the promise; `.await` on it is `await`.
  expect(js).toContain("const first = setTimeout(5, 1);");
  expect(js).toContain("return ((await first) + (await second)) >>> 0;");

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

// ADRs 0032 and 0044: the playground is Rust, React components one per file,
// compiled by rust-js.
test("the playground is Rust components, compiled to the JS main.ts starts", async () => {
  const read = (path: string) => Bun.file(join(target, "playground", path)).text();
  const lib = await read("lib.jsx");
  expect(lib).toContain('import * as app from "./components/app.jsx";');
  expect(lib).toContain("  root.render(\n    <StrictMode>\n      <app.App />\n    </StrictMode>");
  // Each component's file exports it alone, which Fast Refresh needs.
  const components = [
    ["app", "App"], ["editor", "Editor"], ["example_picker", "ExamplePicker"], ["file_item", "FileItem"],
    ["file_tree", "FileTree"], ["pane", "Pane"], ["result_frame", "ResultFrame"], ["stats_table", "StatsTable"],
    ["status_line", "StatusLine"], ["toolbar", "Toolbar"],
  ];
  for (const [file, name] of components) {
    const js = await read(`components/${file}.jsx`);
    expect([...js.matchAll(/^export (?:async )?(?:function|const) (\w+)/gm)].map((m) => m[1])).toEqual([name]);
  }
  // A folder's entries are a FileTree inside it: the component is recursive.
  expect(await read("components/file_tree.jsx")).toContain("<FileTree\n                tree={param[1]._0}\n                depth={(depth + 1) >>> 0}");
  expect(await read("components/file_tree.jsx")).toContain("export function FileTree({ tree, depth, first, selected, onOpen, onDelete }) {");
  // The editor's view is made in an effect, and destroyed in its cleanup.
  expect(await read("components/editor.jsx")).toContain("    return () => {\n      editor.destroy();");
  // A let chain on `&on_submit`, tested where it is: a reference is the value.
  expect(await read("components/editor.jsx")).toContain('    if (onSubmit != null && (e.metaKey || e.ctrlKey) && e.key === "Enter") {');
  // A hook, found by its name, and props as React code names them (ADR 0046).
  expect(await read("dark_mode.js")).toContain("export function useDarkMode() {\n  return useSyncExternalStore(");

  const compiler = await read("compiler.js");
  expect(compiler).toContain("import {\n  ConsoleStdout,\n  Directory,\n  File,\n  OpenFile,\n  PreopenDirectory,\n  WASI,\n} from \"@bjorn3/browser_wasi_shim\";");
  for (const name of ["load", "loadExample", "ms", "mb", "compile"]) {
    expect(compiler).toMatch(new RegExp(`^export (async )?function ${name}\\(`, "m"));
  }
  // The downloads all start before any is awaited.
  expect(compiler).toContain("  const module = loadCompiler(start, stat);\n  const sysroot = loadSysroot(start, stat);");
  expect(compiler).toContain('  const module = await WebAssembly.compileStreaming(window.fetch("./rust-js.wasm"));');
  // A `format!` value shown once, in order, is written in place.
  expect(compiler).toContain("  return `${t.toFixed(0)} ms`;");
  expect(compiler).toContain("  const response = await window.fetch(`./sysroot/${name}`);");
  // A trapped compile is an `Err` (ADR 0035), and `instanceof` a binding.
  expect(compiler).toContain("  const started = $try(() => wasi.start(instance));");
  expect(compiler).toContain('  const ok = started.TAG === "Ok" && started._0 === 0;');
  expect(compiler).toContain("    if (entry instanceof Directory) {");
  // The file tree: sorted with a comparator, a copy of the tree's entries.
  expect(await read("tree.js")).toContain("  let entries = tree.slice();\n  entries.sort((a, b) => {");
  // Linking: a `RegExp`, and `replace` with a closure, for every kind of export.
  const programs = await read("programs.js");
  expect(programs).toContain('  const exports = new RegExp("^export (async function|function|const) (\\\\w+)", "gm");');
  expect(programs).toContain("    const body$1 = body.replace(exports, (_, declared, name) => {");
  // CodeMirror, through imports (ADR 0028), its extensions made once (ADR 0037).
  const codemirror = await read("codemirror.js");
  expect(codemirror).toContain('import { EditorView, basicSetup } from "codemirror";');
  expect(codemirror).toContain("const THEME = new Compartment();");
});

// ADR 0037: `thread_local!` is a variable of its module.
test("thread-locals are module variables", async () => {
  const js = await Bun.file(join(target, "thread_locals.js")).text();
  expect(js).toContain("const COUNT = { value: 0 };\nconst LOG = { value: [] };\nconst START = { value: (Math.imul(10, 4) + 2) | 0 };");
  expect(js).toContain("  COUNT.value = (COUNT.value + 1) >>> 0;\n  return COUNT.value;");
  expect(js).toContain("  })(LOG.value);");
  // A closure that only returns is its body, on the key or its value, in place.
  expect(js).toContain("  return START.value;");
  expect(js).toContain("  return LOG.value.length;");
  // Nothing of std's storage.
  expect(js).not.toContain("__rust_std_internal");
});

// ADR 0036: an iterator is a JS array, and `Ordering` a comparator's number.
test("iterators are array methods, and sorting takes comparators", async () => {
  const js = await Bun.file(join(target, "iterators.js")).text();
  expect(js).toContain("  return $range(0, n).map((i) => Math.imul(i, i) >>> 0);");
  // `|&&x|` is `x`: a reference is the value.
  expect(js).toContain("  return v.filter((x) => x % 2 === 0);");
  expect(js).toContain("  const sum = v.reduce((a, b) => (a + b) | 0, 0);");
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
test("string methods are JS's, and format! is a template literal", async () => {
  const js = await Bun.file(join(target, "strings.js")).text();
  expect(js).toContain("  return `${name}: ${n} item${n === 1 ? \"\" : \"s\"}`;");
  expect(js).toContain("s.startsWith(\"ab\"), s.endsWith(\"c\"), s.includes(\"b/\"), s.includes(\"/\")");
  expect(js).toContain('  return s.replaceAll("/", " / ").replaceAll("a", "A");');
  expect(js).toContain('  return $stripSuffix(file, ".rs") ?? file;');
  expect(js).toContain('  for (const part of path.split("/")) {');
  expect(js).toContain('  const last = path.split("/").at(-1) ?? "";');
  expect(js).toContain('  return pieces.join(" > ");');
  // A string that grows gets a new one each time: JS strings don't change.
  expect(js).toContain('    s += String(i);\n    s += ",";');
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
  expect(js).toContain("  return { TAG: \"Circle\", _0: r };");
  expect(js).toContain("  return { TAG: \"Rect\", w, h };");
  // Matching tests the name, or the `TAG`, then the fields, in place.
  expect(js).toContain('  if (s === "Empty") {\n    return 0;\n  } else if (s.TAG === "Circle") {');
  expect(js).toContain("  } else if ((s.TAG === \"Rect\" && s.w === 0) || s === \"Empty\") {");
  // Through a reference, with no copy: the reference is the value.
  expect(js).toContain("  if (t.TAG === \"Leaf\") {\n    return t._0;\n  } else {\n    return (sum(t._0) + sum(t._1)) | 0;");
  // `Result` is ReScript's `result`.
  expect(js).toContain('  if (match.TAG === "Ok") {\n    return match._0;');
});

// ADR 0031: a `const` is the value rustc computed, under its own name.
test("constants are the values rustc computed, by name", async () => {
  const js = await Bun.file(join(target, "consts.js")).text();
  expect(js).toContain("export const SIZE = 4096;\nconst GREETING = \"hello\";\nconst RATIO = 0.25;\nconst ON = true;");
  expect(js).toContain('const NOTHING = undefined;\nconst LEVEL = "High";');
  // A `const` inside a function goes beside it.
  expect(js).toContain("const STEP = 3;");
  // Each use is a value of its own: copied where it's changed.
  expect(js).toContain("  let p = { ...ORIGIN };\n");
  expect(js).toContain("  return [{ ...p }, { ...ORIGIN }];");
  // A known divisor needs no check for zero.
  expect(js).toContain("  return (SIZE / 1024) >>> 0;");
  // std's are written in place.
  expect(js).toContain("  return [4294967295, -2147483648];");
  expect(js).toContain("  for (const p of PRIMES) {");
});

// ADR 0030: `Some(x)` is `x`, `None` is `undefined`, and `null` counts as `None`.
test("options are the value or undefined", async () => {
  const js = await Bun.file(join(target, "options.js")).text();
  expect(js).toContain("    return (n / 2) | 0;\n  } else {\n    return undefined;");
  // `Some(0)` needs no `!= null`; `Some(n)` does.
  expect(js).toContain("  if (o === 0) {\n    return 100;\n  } else if (o != null && o < 0) {");
  // `if let Some(h) = ..` keeps the value in a `const h`.
  expect(js).toContain("  const h = half(n);\n  if (h != null) {\n    return h;");
  expect(js).toContain("h != null, h == null, h ?? -1");
  // `unwrap_or`'s argument runs even when it isn't needed, as in Rust.
  expect(js).toContain("  const option = half(n);\n  const fallback = bump();\n  const v = option ?? fallback;");
  expect(js).toContain('  return $unwrap(half(n), "an even number");');
  // `==` on options is `==`: `null` from JS equals `undefined` from Rust.
  expect(js).toContain("  return a == b;");
  // `map` puts the closure's body in place, on the option read once.
  expect(js).toContain("    h != null ? double(h) : undefined,\n    h != null ? h > 2 : undefined,");
  expect(js).toContain("    option != null ? Math.imul(option[0], option[1]) : undefined,");
  // Let chains (ADR 0048): one test when the parts need nothing else,
  expect(js).toContain("  const h = half(n);\n  if (h != null && h > 2) {\n    return h;\n  } else {\n    return -1;");
  // an `if` inside for a `let` of a call, only made once the rest held, and
  // then the `else` after both, in a block the `then` leaves.
  expect(js).toContain("  chain: {\n    const h = half(n);\n    if (h != null && h !== 0) {\n      const q = counted(h);\n      if (q != null && q > 1) {\n        v = q;\n        break chain;\n      }\n    }\n    v = 0;\n  }");
  // A closure of statements is called, by a name.
  expect(js).toContain("  const counted = h != null ? map(h) : undefined;");
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
  expect(js).toContain("count$1.value = (count$1.value + by) | 0;");
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


// ADR 0047: a type's methods are an object named after it.
test("methods are their type's object of functions", async () => {
  const js = await Bun.file(join(target, "methods.js")).text();
  expect(js).toContain("export const Counter = {\n  new(step) {\n    return { count: 0, step };\n  }");
  // `self` is named after its type; `&mut self` changes the object itself.
  expect(js).toContain("  tick(counter) {\n    counter.count = (counter.count + counter.step) >>> 0;\n  }");
  expect(js).toContain("      Counter.tick(next);");
  // A call is the method with its receiver first, as Rust's `Counter::tick(&mut c)`.
  expect(js).toContain("  return Counter.value(Counter.ticked(Counter.new(step), times));");
  // Each type's `new` is its own, and the object ends with a blank line.
  expect(js).toContain("};\n\nexport const Pair = {\n  new(a, b) {");
});

test("methods across modules, in a thread-local, and camelCase", async () => {
  const { fixture, compiler } = await import("./support");
  const { writeFileSync } = await import("node:fs");
  const dir = fixture("methods");
  writeFileSync(join(dir, "lib.rs"), `#![rust_js::camel_case]
use std::cell::Cell;

mod shapes;

thread_local! {
    static SIDE: Cell<u32> = Cell::new(shapes::Square::new(3).side_length());
}

pub fn area_of(side: u32) -> u32 {
    shapes::Square::new(side).area()
}

pub fn first_side() -> u32 {
    SIDE.get()
}
`);
  writeFileSync(join(dir, "shapes.rs"), `pub struct Square {
    pub side: u32,
}

impl Square {
    pub fn new(side: u32) -> Square {
        Square { side }
    }

    pub fn side_length(&self) -> u32 {
        self.side
    }

    pub fn area(&self) -> u32 {
        self.side_length() * self.side_length()
    }
}

/// Only this module uses it, so its object isn't exported.
struct Tally(u32);

impl Tally {
    fn doubled(&self) -> u32 {
        self.0 * 2
    }
}

pub fn tally_of_four() -> u32 {
    Tally(4).doubled()
}
`);
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
  const lib = await Bun.file(join(dir, "lib.js")).text();
  const shapes = await Bun.file(join(dir, "shapes.js")).text();
  expect(lib).toContain("const SIDE = { value: shapes.Square.sideLength(shapes.Square.new(3)) };");
  expect(lib).toContain("  return shapes.Square.area(shapes.Square.new(side));");
  expect(shapes).toContain("export const Square = {\n  new(side) {");
  expect(shapes).toContain("  area(square) {\n    return Math.imul(Square.sideLength(square), Square.sideLength(square)) >>> 0;");
  expect(shapes).toContain("\nconst Tally = {\n  doubled(tally) {\n    return Math.imul(tally[0], 2) >>> 0;\n  },\n};\n\nexport function tallyOfFour() {");
  // Laid out on lines of its own, a lone method still maps back to its Rust.
  const { decodeMappings, lookup } = await import("./sourcemap.ts");
  const segments = decodeMappings((await Bun.file(join(dir, "shapes.js.map")).json()).mappings);
  const jsLines = shapes.split("\n");
  const rsLines = (await Bun.file(join(dir, "shapes.rs")).text()).split("\n");
  for (const [jsText, rustText] of [["Math.imul(tally[0], 2)", "self.0 * 2"], ["doubled(tally) {", "doubled(&self)"]]) {
    const line = jsLines.findIndex((l) => l.includes(jsText));
    const hit = lookup(segments, line, jsLines[line].indexOf(jsText));
    expect([jsText, hit && rsLines[hit.srcLine].slice(hit.srcCol).startsWith(rustText)]).toEqual([jsText, true]);
  }
  const module = await import(join(dir, "lib.js"));
  expect(module.areaOf(4)).toBe(16);
  expect(module.firstSide()).toBe(3);
  expect((await import(join(dir, "shapes.js"))).tallyOfFour()).toBe(8);
});

// ADR 0019: a module is imported when anything of it is used, a `const` or
// a `thread_local!` too; a module that isn't used takes no name from locals.
test("imports follow what's used: functions, consts and thread-locals", async () => {
  const { fixture, compiler } = await import("./support");
  const { writeFileSync } = await import("node:fs");
  const dir = fixture("imports");
  writeFileSync(join(dir, "lib.rs"), `mod editor;
mod helpers;
mod util;

pub fn sized() -> u32 {
    util::SIZE + util::COUNT.get() + helpers::one()
}

pub fn doubled(editor: u32) -> u32 {
    editor * 2
}
`);
  writeFileSync(join(dir, "util.rs"), `use std::cell::Cell;

pub const SIZE: u32 = 4;

thread_local! {
    pub static COUNT: Cell<u32> = Cell::new(3);
}
`);
  writeFileSync(join(dir, "helpers.rs"), "pub fn one() -> u32 {\n    1\n}\n");
  writeFileSync(join(dir, "editor.rs"), "pub fn open() -> u32 {\n    1\n}\n");
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
  const lib = await Bun.file(join(dir, "lib.js")).text();
  expect(lib).toContain('import * as util from "./util.js";');
  expect(lib).not.toContain("editor.js");
  expect(lib).toContain("export function doubled(editor) {");
  const module = await import(join(dir, "lib.js"));
  expect(module.sized()).toBe(8);
  expect(module.doubled(3)).toBe(6);
});

// ADR 0020: only a mutated type is copied, and that's decided per type: a
// mutated `Pair<u32>` isn't a reason to copy a `Pair<bool>`. A generic
// function that mutates `Holder<T>` may mutate any `Holder<..>`, though.
test("copies are made for the mutated instantiations of a generic type only", async () => {
  const { fixture, compiler } = await import("./support");
  const { writeFileSync } = await import("node:fs");
  const dir = fixture("copies");
  writeFileSync(join(dir, "lib.rs"), `#[derive(Clone, Copy)]
pub struct Pair<T: Copy> {
    pub a: T,
    pub b: T,
}

#[derive(Clone, Copy)]
pub struct Holder<T: Copy> {
    pub value: T,
}

pub fn bumped(p: Pair<u32>) -> (Pair<u32>, Pair<u32>) {
    let mut q = p;
    q.a += 1;
    (p, q)
}

pub fn twice(p: Pair<bool>) -> (Pair<bool>, Pair<bool>) {
    let q = p;
    (p, q)
}

pub fn set<T: Copy>(holder: &mut Holder<T>, value: T) {
    holder.value = value;
}

pub fn both(h: Holder<bool>) -> (Holder<bool>, Holder<bool>) {
    let mut g = h;
    set(&mut g, !h.value);
    (h, g)
}
`);
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
  const js = await Bun.file(join(dir, "lib.js")).text();
  expect(js).toContain("export function bumped(p) {\n  let q = { ...p };");
  expect(js).toContain("export function twice(p) {\n  const q = p;");
  expect(js).toContain("export function both(h) {\n  let g = { ...h };");
  const module = await import(join(dir, "lib.js"));
  expect(module.bumped({ a: 1, b: 2 })).toEqual([{ a: 1, b: 2 }, { a: 2, b: 2 }]);
  expect(module.twice({ a: true, b: false })).toEqual([{ a: true, b: false }, { a: true, b: false }]);
  expect(module.both({ value: true })).toEqual([{ value: true }, { value: false }]);
});

// ADR 0051: `Option<T>` in generic code boxes only what could look like `None`.
test("an Option of a generic T is its value, boxed only when that looks like None", async () => {
  const js = await Bun.file(join(target, "generic_options.js")).text();
  expect(js).toContain("export function pick(x, keep) {\n  if (keep) {\n    return $some(x);");
  expect(js).toContain("  return $someValue(pick(x, keep) ?? $some(fallback));");
  expect(js).toContain("  return option != null ? $some(f($someValue(option))) : undefined;");
  expect(js).toContain("  return $pop(xs);");
  // Code that isn't generic keeps `Some(x)` as `x`.
  expect(await Bun.file(join(target, "options.js")).text()).not.toContain("$some");
  // A JS caller gets plain values, and a box only for what's `None`-like.
  expect(genericOptions.pick(5, true)).toBe(5);
  expect(genericOptions.pick("a", true)).toBe("a");
  expect(genericOptions.pick(undefined, true)).toEqual({ $someNone: 0 });
  expect(genericOptions.pick(undefined, false)).toBeUndefined();
});

// ADR 0052: the crate's own `Default`, `From` and `Clone`. A clone is a copy
// only of what could be told apart, and a hand-written one is called.
test("std trait impls are direct calls, and a clone copies only what changes", async () => {
  const js = await Bun.file(join(target, "std_traits.js")).text();
  // Hand-written: called where the type is known, a dictionary for a generic.
  expect(js).toContain("const b = trackedClone_clone(a);");
  expect(js).toContain("const both = twice(copy.tracked, trackedClone());");
  expect(js).toContain("export function twice(x, TClone) {\n  return [TClone.clone(x), TClone.clone(x)];");
  // Derived: written in place, copying only the `Vec` that's pushed to.
  expect(js).toContain("  let t = { ...s, tags: s.tags.slice() };");
  expect(js).toContain("  const dot = \"Dot\";");
  // `From`, once per argument type, and `into()` is the same call.
  expect(js).toContain("const b = metersFromU32_from(3);");
  expect(js).toContain("const c = metersFromF64_from(1.5);");

  const { fixture, compiler } = await import("./support");
  const { writeFileSync } = await import("node:fs");
  const dir = fixture("clones");
  writeFileSync(join(dir, "lib.rs"), `#[derive(Clone)]
pub struct Point {
    pub x: u32,
}

pub fn read_only(v: &Vec<u32>, p: &Point) -> (Vec<u32>, Point) {
    (v.clone(), p.clone())
}
`);
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
  // Nothing changes a `Vec<u32>` or a `Point`: a clone is the value itself.
  expect(await Bun.file(join(dir, "lib.js")).text()).toContain("  return [v, p];");
});

// ADR 0053: `==` is `===` for JS primitives and `$eq` for what compares field
// by field, until a hand-written `eq` is in it: then it's called, part by part.
test("== calls a hand-written eq wherever it's inside, and generics take a dictionary", async () => {
  const js = await Bun.file(join(target, "std_traits.js")).text();
  expect(js).toContain("export function same(a, b, TPartialEq) {\n  return TPartialEq.eq(a, b);");
  expect(js).toContain("    versionPartialEq_eq(r1.version, r2.version) && $eq(r1.notes, r2.notes),");
  expect(js).toContain("    !(versionPartialEq_eq(r1.version, r3.version) && $eq(r1.notes, r3.notes)),");
  expect(js).toContain("    left.TAG === \"Bump\"\n      ? right.TAG === \"Bump\" && versionPartialEq_eq(left._0, right._0)\n      : $eq(left, right");
  // A fieldless variant is a string: only itself is equal to it.
  expect(js).toContain("} === \"Nothing\"");
  // A `T: Eq` is given `T`'s `PartialEq`, and one that compares field by field is `$eq`.
  expect(js).toContain("count_equal(all, version(1, \"z\"), versionPartialEq())");
  expect(js).toContain("{ eq: $eq }");
  // `!=` of a hand-written `PartialEq<f64>` negates its `eq`.
  expect(js).toContain("return [metersPartialEqF64_eq(m, 2), !metersPartialEqF64_eq(m, 3");
});

// ADR 0054: a `fmt` returns the string it writes, and `{}` of a value calls it.
test("a Display impl's fmt returns the string it writes", async () => {
  const js = await Bun.file(join(target, "std_traits.js")).text();
  // One write: its string. One per way through: a `return` each.
  expect(js).toContain("function pointDisplay_fmt(point) {\n  return `(${point.x}, ${point.y})`;\n}");
  expect(js).toContain("  if (figure === \"Dot\") {\n    return \"a dot\";\n  } else {\n    return `a polygon of ${figure._0.length}`;");
  // More: a string built up, with nested `fmt`s and a helper that writes.
  expect(js).toContain('    f += pointDisplay_fmt(stop);');
  expect(js).toContain('    f += write_loop(route.stops.length);');
  expect(js).toContain("function write_loop(stops) {\n  return ` (a loop of ${stops})`;");
  // `{}` of one, and generics given a dictionary.
  expect(js).toContain("return `${labeled.label}: ${TDisplay.fmt(labeled.value)}`;");
  expect(js).toContain("export function shown(x, TDisplay) {\n  return `<${TDisplay.fmt(x)}>`;");
  expect(js).toContain("{ fmt: $displayF64 }");
});

// ADR 0055: an iterator of the crate's own is a JS iterator, with lazy helpers.
test("an Iterator impl is a JS iterator, lazy until something wants all of it", async () => {
  const js = await Bun.file(join(target, "std_traits.js")).text();
  expect(js).toContain("for (const x of $iterator({ n: 3 }, countdownIterator_next)) {");
  expect(js).toContain("const first = countdownIterator_next(c) ?? 0;");
  // Endless, so only lazy helpers: `drop`, `take`, `find`.
  expect(js).toContain("$iterator(fibonacci(), fibonacciIterator_next).drop(1).take(6).toArray()");
  expect(js).toContain("$iterator(fibonacci(), fibonacciIterator_next).find((x) => x > 50)");
  expect(js).toContain("$iterator({ n: 4 }, countdownIterator_next).toArray().length");
  // A generic `next` may box a `Some` that looks like `None`: unboxed as it comes out.
  expect(js).toContain("      (iterator) => repeatIterator_next(iterator, { clone: (value) => value }),\n      true,\n    )");
});

// `format_args!` is recognized whole, so its arguments are written in place,
// with `const`s only where the order Rust runs them in would change.
test("format! writes its arguments in place, in the order Rust runs them", async () => {
  const js = await Bun.file(join(target, "strings.js")).text();
  // Named arguments come after the others in Rust, but none has effects.
  expect(js).toContain("return `${name}: ${n} item${n === 1 ? \"\" : \"s\"}`;");
  // Shown out of order, with effects: in `const`s first, in Rust's order.
  expect(js).toContain(
    "  const arg = tick(c);\n  const arg$1 = tick(c);\n  const arg$2 = c.value;\n" +
      "  return `${arg$1} ${arg} ${arg} ${arg$2}`;",
  );
  // And a call in one doesn't make the call before it a `const`.
  const traits = await Bun.file(join(root, "test/snapshots/traits/traits.js")).text();
  expect(traits).toContain("label: (self) => `${circleShape_name(self)} of area ${$displayF64(circleShape_area(self))}`");
});

// ADR 0057: an `Ordering` is -1, 0 or 1; derived, the fields in turn with `||`.
test("PartialOrd and Ord compare with $cmp, a hand-written cmp, or the parts in turn", async () => {
  const js = await Bun.file(join(target, "std_traits.js")).text();
  expect(js).toContain("all.sort((a, b) => $cmp(a.major, b.major) || $cmp(a.minor, b.minor));");
  expect(js).toContain("words.sort(wordOrd_cmp);");
  expect(js).toContain("lists.sort((a, b) => $cmpItems(a, b, $cmp));");
  // Generic: dictionaries, `{ cmp }` and `{ partial_cmp }`.
  expect(js).toContain("export function in_order(a, b, TPartialOrd) {\n  return TPartialOrd.partial_cmp(a, b) <= 0;");
  // `NaN` isn't ordered: `$thenCmp` stops at an `undefined`, which `||` wouldn't.
  expect(js).toContain("$thenCmp($partialCmp(p.x, q.x), $partialCmp(p.y, q.y)) < 0");
});

// ADR 0058: format options, where Rust applies them.
test("format options pad, round and change base as Rust does", async () => {
  const js = await Bun.file(join(target, "strings.js")).text();
  // Numbers are ASCII: JS's own padding. Strings count `char`s: `$pad`.
  expect(js).toContain("`[${String(n).padStart(6)}] [${String(n).padEnd(6)}] [${$pad(name, 9, \"^\")}]");
  expect(js).toContain('$pad(name, 9, ">", "*")');
  expect(js).toContain("[0x${(n >>> 0).toString(16)}] [");
  // `{:.1}` rounds a tie to even, exactly, and `{:?}` of an `f64` keeps its `.0`.
  expect(js).toContain("return `${$toFixed(x, 0)} ${$toFixed(x, 1)} ${$toFixed(x, 3).padStart(8)} ${$debugF64(x)}`;");
});

// ADR 0059: a `HashMap` is a JS `Map`, and a `HashSet` a `Set`.
test("HashMap and HashSet are a JS Map and Set", async () => {
  const js = await Bun.file(join(target, "collections.js")).text();
  // The count idiom: the value there, or the one it would start as.
  expect(js).toContain("const current = $orInsert(counts, word, 0);\n    counts.set(word, (current + 1) >>> 0);");
  // A value that's used is the old one; one that isn't is plain `set`.
  expect(js).toContain('  m.set("a", n);\n  const old = $insert(m, "a", (n + 1) >>> 0);');
  expect(js).toContain("$orInsertWith(groups, key, () => []).push(i);");
  expect(js).toContain("const copy = new Map(Array.from(groups).map(([key, value]) => [key, value.slice()]));");
});

// ADR 0060: `{:?}` by the type, and a derived `Debug` is a function of its own.
test("a derived Debug is a function, left out unless something shows the type", async () => {
  const js = await Bun.file(join(target, "std_traits.js")).text();
  expect(js).toContain("function posDebug_fmt(pos) {\n  return `Pos { x: ${$debugF64(pos.x)}, y: ${$debugF64(pos.y)} }`;\n}");
  expect(js).toContain("  if (glyph === \"Dot\") {\n    return \"Dot\";\n  } else if (glyph.TAG === \"Ring\") {\n    return `Ring(${$debugF64(glyph._0)})`;");
  // Generic: `T`'s `fmt`, from a dictionary.
  expect(js).toContain("export function debugged(x, TDebug) {\n  return TDebug.fmt(x);");
  // Derived, and never shown: not in the JS at all.
  expect(js).not.toContain("neverShown");
});

// ADR 0063: a `char`'s questions are regular expressions of the Unicode
// properties Rust uses, and `parse` is a `Result` whose `Err` is Rust's message.
test("chars, parse and slices are plain JS with Rust's answers", async () => {
  const js = await Bun.file(join(target, "text.js")).text();
  expect(js).toContain("/^\\p{White_Space}$/u.test(c),\n    /^\\p{Alphabetic}$/u.test(c),");
  expect(js).toContain("const code = c.codePointAt(0);");
  expect(js).toContain("String.fromCharCode(65)");
  // `map_err(|e| e.to_string())` of a fresh `Result`: the message already is one.
  expect(js).toContain("const n = $parseInt(s, 0, 4294967295);");
  expect(js).toContain('text.split(/\\p{White_Space}+/u).filter((word) => word !== "")');
  expect(js).toContain("$slice(v, 1, 3)");
});

// A program that reads text: loops take tuples apart as JS does, and a
// value `{:?}` shows by its parts gets a name first.
test("the calculator's JS is what a person would write", async () => {
  const js = await Bun.file(join(target, "calc.js")).text();
  expect(js).toContain("for (const [i, c] of Array.from(s).entries()) {");
  expect(js).toContain('const arg$1 = first_dup("abcdbe");');
  expect(js).toContain("`Some((${arg$1[0]}, ${$debugStr(arg$1[1], \"'\")}))`");
  expect(js).toContain('$splitBy(text, (c) => !/^[\\p{Alphabetic}\\p{N}]$/u.test(c))');
  // `?` from a `&str` error to a `String` one: the same string, returned as it is.
  expect(js).toContain("_0: \"underflow\" };\n      if (result$1.TAG === \"Err\") {\n        return result$1;");
});

// ADR 0064: a number's methods are `Math`'s where JS agrees with Rust, and
// a helper where it doesn't; an operator is its impl's function.
test("numbers are Math's, operators call their impl, and vec![x; n] fills", async () => {
  const js = await Bun.file(join(target, "numbers.js")).text();
  expect(js).toContain("return Math.sqrt(vec2.x * vec2.x + vec2.y * vec2.y);");
  expect(js).toContain("vec2Add_add(");
  expect(js).toContain("`${$displayF64(Math.floor(x))} ${$displayF64(Math.ceil(x))} ${$displayF64($round(x))}");
  expect(js).toContain("$checked(b - 10, 0, 4294967295)");
  expect(js).toContain(" ${Math.max(b - 100, 0)} ");
  // Each row made again; a struct cloned, since one is changed later.
  expect(js).toContain("Array.from({ length: n }, () => new Array(n).fill(0))");
  expect(js).toContain("Array.from({ length: 3 }, () => ({ ...cell }))");
  expect(js).toContain("for (const [j$1, v] of row.entries()) {");
  // Numbers as JS writes them, and constants shown as their text.
  expect(js).toContain("$displayF64(2.220446049250313e-16)");
  expect(js).not.toContain("((tuple) =>");
});

// ADR 0067: `let ... else`, range patterns and `@`, and a `&mut` to a
// map's number, which a write puts back.
test("the store's JS: let-else, ranges, and writes through a map's value", async () => {
  const js = await Bun.file(join(target, "inventory.js")).text();
  expect(js).toContain("let have = store.stock.get(e.item);\n      if (have == null) {\n        return { TAG: \"Err\", _0: `unknown item ${e.item}` };\n      }");
  expect(js).toContain("have = (have - e.qty) >>> 0;\n      store.stock.set(e.item, have);");
  expect(js).toContain("} else if (x >= 1 && x < 10) {\n      return `few ${x}`;\n    } else if ((x >= 10 && x <= 99) || x >= 200) {");
  // A bound at the type's own end always holds, so it isn't tested.
  expect(js).toContain("if (n <= -1) {");
  expect(js).toContain("? `revenue ${$toFixed(store.revenue, 2)} under ${$toFixed(revenue[0], 2)}`\n    : undefined;");
  // A block's statements go before the `const` of its value.
  expect(js).toContain("  const c = counter;\n  const bump = (n) => {");
});

// ADR 0070: a std function taken as a value is an arrow, and an array's
// constant index below its length is read directly.
test("the report's JS: function values, case mapping, and plain array reads", async () => {
  const js = await Bun.file(join(target, "report.js")).text();
  expect(js).toContain('const parts = line.split(",").map((s) => s.trim());');
  expect(js).toContain(".flatMap((c) => Array.from(c.toUpperCase()))");
  expect(js).toContain('.map((c) => /^\\p{White_Space}$/u.test(c))');
  expect(js).toContain(".map((n) => Math.sqrt(n))");
  expect(js).toContain("HEADERS[0]");
  expect(js).toContain('$debugParseError($unwrapErr($parseInt("", 0, 255)), "ParseIntError")');
});

// ADR 0061: `impl Iterator` is the type it hides, and a generic iterator is
// whatever JS iterable it's given.
test("generic iterators take arrays and JS iterators alike", async () => {
  const js = await Bun.file(join(target, "std_traits.js")).text();
  expect(js).toContain("export function evens_below(n) {\n  return $range(0, n).filter((x) => x % 2 === 0);");
  expect(js).toContain("export function middle(items, k) {\n  return Iterator.from(items).drop(1).take(k).toArray();");
  expect(js).toContain("for (const x of items) {");
  // One of the crate's own, given where a generic one goes.
  expect(js).toContain("total($iterator({ n }, countdownIterator_next))");
});
