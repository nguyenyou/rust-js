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
 * Build the web crate's metadata for the playground's target (ADR 0024), with
 * the pinned rustc: every program is compiled with `--extern web=` this file.
 */
export function buildWebCrate(out: string) {
  mkdirSync(dirname(out), { recursive: true });
  const script = join(import.meta.dir, "../../web/build.sh");
  const p = Bun.spawnSync([script, "--target", "wasm32-unknown-unknown", "-o", out], { stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`web/build.sh failed:\n${p.stderr.toString()}`);
}

/**
 * Build the react crate's metadata for the page's own Rust (ADR 0044), for
 * the React the page has installed, with the web crate's beside it:
 * `<dir>/libreact.rmeta` and `<dir>/libweb.rmeta`.
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

/**
 * What the page fetches beside itself (ADR 0045): the compiler, the sysroot's
 * metadata, the web crate's, and the examples. Served by Vite's dev server,
 * and put in the build as they are, unhashed, since the page asks for them
 * by name.
 */
export function playgroundFiles(): Plugin {
  const webCrate = join(import.meta.dir, "../../target/web/libweb.rmeta");
  // The file at a path the page asks for, or the JSON to send.
  function served(path: string): { file: string } | { json: unknown } | undefined {
    const sysroot = sysrootFiles();
    if (path === "/rust-js.wasm") return { file: wasmPath };
    if (path === "/sysroot.json") return { json: sysroot };
    const name = path.match(/^\/sysroot\/([^/]+)$/)?.[1];
    if (name && sysroot.includes(name)) return { file: join(sysrootDir, name) };
    if (path === "/web/libweb.rmeta") return { file: webCrate };
    if (path === "/web/libreact.rmeta") return { file: join(dirname(webCrate), "libreact.rmeta") };
    if (path === "/examples.json") return { json: examplesManifest() };
    // Only files an example lists: never an arbitrary path.
    const [example, ...rest] = path.startsWith("/examples/") ? path.slice("/examples/".length).split("/") : [];
    const found = examples().find((e) => e.name === example);
    const file = rest.join("/");
    if (found?.files.includes(file)) return { file: join(found.dir, file) };
  }
  // Every path the build needs, for `generateBundle`.
  function all(): string[] {
    const paths = ["/rust-js.wasm", "/sysroot.json", "/web/libweb.rmeta", "/web/libreact.rmeta", "/examples.json"];
    paths.push(...sysrootFiles().map((name) => `/sysroot/${name}`));
    for (const example of examples()) paths.push(...example.files.map((file) => `/examples/${example.name}/${file}`));
    return paths;
  }
  return {
    name: "playground-files",
    buildStart() {
      buildReactCrate(dirname(webCrate));
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
