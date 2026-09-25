// What the playground page needs besides itself, shared by the dev server
// (serve.ts) and the static build for GitHub Pages (build.ts).

import { readdirSync } from "node:fs";
import { join } from "node:path";

export const wasmPath = join(import.meta.dir, "../target/wasm32-wasip1/release/rust-js.wasm");
export const examplePath = join(import.meta.dir, "../../examples/fib.rs");
export const sysrootDir = join(import.meta.dir, "../sysroot/lib/rustlib/wasm32-unknown-unknown/lib");

// The crates rustc loads to type-check a program against `std`. Found by
// removing files one at a time until compiling failed; the rest of the
// sysroot (test, proc_macro, getopts, the backtrace crates, ...) isn't read.
const NEEDED = [
  "adler2", "alloc", "cfg_if", "compiler_builtins", "core", "dlmalloc", "hashbrown", "libc",
  "miniz_oxide", "rustc_demangle", "rustc_std_workspace_alloc", "rustc_std_workspace_core",
  "std", "std_detect", "unwind",
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
