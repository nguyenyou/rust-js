// What the playground page needs besides itself, shared by the dev server
// (serve.ts) and the static build for GitHub Pages (build.ts).

import { mkdirSync, readdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";

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
