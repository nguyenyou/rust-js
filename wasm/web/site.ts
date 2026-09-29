// What the playground page needs besides itself: the files it fetches, for
// Vite's dev server and the static build (vite.config.ts), and the crates
// its own Rust and its users' programs are compiled with.

import { mkdirSync, readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, relative } from "node:path";

import type { Plugin } from "vite";

export const wasmPath = join(import.meta.dir, "../target/wasm32-wasip1/release/rust-js.wasm");
export const sysrootDir = join(import.meta.dir, "../sysroot/lib/rustlib/wasm32-unknown-unknown/lib");
const examplesDir = join(import.meta.dir, "../../examples");

// The crates rustc loads to type-check a program against `std`. Found by
// removing files one at a time until compiling failed; the rest of the
// sysroot (test, proc_macro, getopts, the backtrace crates, ...) isn't read.
const NEEDED = [
  "adler2", "alloc", "cfg_if", "compiler_builtins", "core", "dlmalloc", "hashbrown", "libc",
  "miniz_oxide", "rustc_demangle", "rustc_std_workspace_alloc", "rustc_std_workspace_core",
  "std", "std_detect", "unwind",
  // `--test` (the Test button) also needs libtest and what it pulls in.
  "getopts", "panic_abort", "rustc_std_workspace_std", "test",
];

/** File names, in `sysrootDir`, of the metadata the page downloads. */
export function sysrootFiles(): string[] {
  const files = readdirSync(sysrootDir).filter((f) =>
    NEEDED.some((crate) => f.startsWith(`lib${crate}-`) && f.endsWith(".rmeta")),
  );
  if (files.length !== NEEDED.length) {
    throw new Error(`expected ${NEEDED.length} sysroot files, found ${files.length}; run ../build.sh`);
  }
  return files;
}

/**
 * Build the webapi crate's metadata for the playground's target (ADR 0024), and
 * the js crate's beside it (ADR 0102), with the pinned rustc: every program is
 * compiled with `--extern webapi=` this file.
 */
export function buildWebapiCrate(out: string) {
  mkdirSync(dirname(out), { recursive: true });
  const script = join(import.meta.dir, "../../webapi/build.sh");
  const p = Bun.spawnSync([script, "-o", out, "--target", "wasm32-unknown-unknown"], { stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`webapi/build.sh failed:\n${p.stderr.toString()}`);
}

/**
 * Build the react crate's metadata for the page's own Rust (ADR 0044), for
 * the React the page has installed, with the webapi and js crates' beside it:
 * `<dir>/libreact.rmeta`, `<dir>/libwebapi.rmeta` and `<dir>/libjs.rmeta`.
 */
export function buildReactCrate(dir: string) {
  mkdirSync(dir, { recursive: true });
  const react = JSON.parse(readFileSync(createRequire(import.meta.path).resolve("react/package.json"), "utf8")).version;
  const script = join(import.meta.dir, "../../react/build.sh");
  const p = Bun.spawnSync([script, "-o", join(dir, "libreact.rmeta"), "--react", react, "--target", "wasm32-unknown-unknown"], { stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`react/build.sh failed:\n${p.stderr.toString()}`);
}

/** A crate the page can load: its files, relative to `dir`, and its root. */
export type Example = { name: string; title: string; root: string; files: string[]; dir: string };

/** The examples, read from ../../examples. The first one loads by default. */
export function examples(): Example[] {
  const modulesDir = join(examplesDir, "modules");
  const modulesFiles = readdirSync(modulesDir, { recursive: true, encoding: "utf8" })
    .filter((f) => f.endsWith(".rs"))
    .map((f) => relative(modulesDir, join(modulesDir, f)).replaceAll("\\", "/"))
    .sort();
  return [
    { name: "todo", title: "Todo list (DOM, Vec, RefCell)", root: "todo.rs", files: ["todo.rs"], dir: examplesDir },
    { name: "counter", title: "Counter (DOM, closures)", root: "counter.rs", files: ["counter.rs"], dir: examplesDir },
    { name: "react_counter", title: "Counter (React, JSX)", root: "react_counter.rs", files: ["react_counter.rs"], dir: examplesDir },
    { name: "todomvc", title: "TodoMVC (React, routes, localStorage)", root: "lib.rs", files: ["lib.rs", "item.rs", "model.rs"], dir: join(examplesDir, "todomvc") },
    { name: "countdown", title: "Countdown (async, await)", root: "countdown.rs", files: ["countdown.rs"], dir: examplesDir },
    { name: "fetch", title: "Fetch (async, the network)", root: "fetch.rs", files: ["fetch.rs"], dir: examplesDir },
    { name: "modules", title: "Modules (a crate across files)", root: "lib.rs", files: modulesFiles, dir: modulesDir },
    { name: "structs", title: "Structs and tuples", root: "structs.rs", files: ["structs.rs"], dir: examplesDir },
    { name: "enums", title: "Enums with fields", root: "enums.rs", files: ["enums.rs"], dir: examplesDir },
    { name: "strings", title: "Strings", root: "strings.rs", files: ["strings.rs"], dir: examplesDir },
    { name: "results", title: "Result and ?", root: "results.rs", files: ["results.rs"], dir: examplesDir },
    { name: "iterators", title: "Iterators and sorting", root: "iterators.rs", files: ["iterators.rs"], dir: examplesDir },
    { name: "thread_locals", title: "thread_local!", root: "thread_locals.rs", files: ["thread_locals.rs"], dir: examplesDir },
    { name: "options", title: "Option", root: "options.rs", files: ["options.rs"], dir: examplesDir },
    { name: "consts", title: "const items", root: "consts.rs", files: ["consts.rs"], dir: examplesDir },
    { name: "closures", title: "Closures", root: "closures.rs", files: ["closures.rs"], dir: examplesDir },
    { name: "collections", title: "Vec, for loops, RefCell", root: "collections.rs", files: ["collections.rs"], dir: examplesDir },
    { name: "fib", title: "fib (one file)", root: "fib.rs", files: ["fib.rs"], dir: examplesDir },
  ];
}

/** What the page's `examples.json` holds: everything but local paths. */
export function examplesManifest() {
  return examples().map(({ name, title, root, files }) => ({ name, title, root, files }));
}

/** The stylesheets a program may import (ADR 0028): TodoMVC's look. */
const STYLES = ["todomvc-common/base.css", "todomvc-app-css/index.css"];

/** What a program may import of the page's: React's modules, and stylesheets. */
export async function framePackages(): Promise<{ modules: Record<string, string>; styles: Record<string, string> }> {
  const require = createRequire(import.meta.path);
  const styles = Object.fromEntries(STYLES.map((specifier) => [specifier, readFileSync(require.resolve(specifier), "utf8")]));
  return { modules: await reactModules(), styles };
}

/** The specifiers of React's a program imports, each a module of the bundle's. */
const REACT = { react: "react", "react/jsx-runtime": "jsxRuntime", "react-dom/client": "reactDomClient" };

/**
 * React, for the Result frame to run a React program (ADR 0044): the React
 * this page has installed, as one module, so there's one React, and a
 * module for each specifier a program imports, of its names. By specifier:
 * the page links each into the frame's import map, beside the program's.
 */
export async function reactModules(): Promise<Record<string, string>> {
  const dir = join(import.meta.dir, "../../target/playground-react");
  mkdirSync(dir, { recursive: true });
  const require = createRequire(import.meta.path);
  const entry = join(dir, "entry.js");
  const lines = Object.entries(REACT).map(([specifier, name]) => `export * as ${name} from ${JSON.stringify(require.resolve(specifier))};`);
  await Bun.write(entry, lines.join("\n") + "\n");
  const built = await Bun.build({
    entrypoints: [entry], target: "browser", format: "esm", minify: true,
    define: { "process.env.NODE_ENV": JSON.stringify("production") },
  });
  if (!built.success) throw new Error(`React for the Result frame: ${built.logs.join("\n")}`);
  const modules: Record<string, string> = { "react-bundle": await built.outputs[0].text() };
  for (const [specifier, name] of Object.entries(REACT)) {
    const names = Object.keys(await import(require.resolve(specifier))).filter((n) => /^[A-Za-z_$][\w$]*$/.test(n) && n !== "default");
    modules[specifier] = `import { ${name} as all } from "react-bundle";\nexport const { ${names.join(", ")} } = all;\nexport default all;\n`;
  }
  return modules;
}

/**
 * What the page fetches beside itself (ADR 0045): the compiler, the sysroot's
 * metadata, the webapi, js and react crates', and the examples. Served by Vite's dev server,
 * and put in the build as they are, unhashed, since the page asks for them
 * by name.
 */
export function playgroundFiles(): Plugin {
  const webapiCrate = join(import.meta.dir, "../../target/crates/libwebapi.rmeta");
  let packages: { modules: Record<string, string>; styles: Record<string, string> } = { modules: {}, styles: {} };
  // The file at a path the page asks for, or the JSON to send.
  function served(path: string): { file: string } | { json: unknown } | undefined {
    const sysroot = sysrootFiles();
    if (path === "/rust-js.wasm") return { file: wasmPath };
    if (path === "/sysroot.json") return { json: sysroot };
    const name = path.match(/^\/sysroot\/([^/]+)$/)?.[1];
    if (name && sysroot.includes(name)) return { file: join(sysrootDir, name) };
    if (path === "/crates/libwebapi.rmeta") return { file: webapiCrate };
    if (path === "/crates/libreact.rmeta") return { file: join(dirname(webapiCrate), "libreact.rmeta") };
    if (path === "/crates/libjs.rmeta") return { file: join(dirname(webapiCrate), "libjs.rmeta") };
    // What the programs it compiles import (ADR 0103).
    if (path === "/runtime.js") return { file: join(import.meta.dir, "../../runtime/index.js") };
    if (path === "/packages.json") return { json: packages };
    if (path === "/examples.json") return { json: examplesManifest() };
    // Only files an example lists: never an arbitrary path.
    const [example, ...rest] = path.startsWith("/examples/") ? path.slice("/examples/".length).split("/") : [];
    const found = examples().find((e) => e.name === example);
    const file = rest.join("/");
    if (found?.files.includes(file)) return { file: join(found.dir, file) };
  }
  // Every path the build needs, for `generateBundle`.
  function all(): string[] {
    const paths = ["/rust-js.wasm", "/sysroot.json", "/crates/libwebapi.rmeta", "/crates/libjs.rmeta", "/crates/libreact.rmeta", "/runtime.js", "/packages.json", "/examples.json"];
    paths.push(...sysrootFiles().map((name) => `/sysroot/${name}`));
    for (const example of examples()) paths.push(...example.files.map((file) => `/examples/${example.name}/${file}`));
    return paths;
  }
  return {
    name: "playground-files",
    async buildStart() {
      buildReactCrate(dirname(webapiCrate));
      packages = await framePackages();
    },
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const found = served(decodeURIComponent(new URL(req.url ?? "/", "http://localhost").pathname));
        if (!found) return next();
        if ("json" in found) {
          res.setHeader("Content-Type", "application/json");
          res.end(JSON.stringify(found.json));
        } else {
          if (found.file.endsWith(".wasm")) res.setHeader("Content-Type", "application/wasm");
          res.end(readFileSync(found.file));
        }
      });
    },
    generateBundle() {
      for (const path of all()) {
        const found = served(path)!;
        const source = "json" in found ? JSON.stringify(found.json) : readFileSync(found.file);
        this.emitFile({ type: "asset", fileName: path.slice(1), source });
      }
    },
  };
}
