// Compile the playground's own Rust (./rust/lib.rs) to JS, beside it, with
// rust-js.wasm: the compiler the page runs, here under the same WASI shim,
// in Bun. build.ts and serve.ts run this before bundling main.ts, which
// imports the result.
//
//   /wasm/web/rust/...   the crate, and where lib.js and lib.js.map go
//   /sysroot/...         the std metadata rustc type-checks against
//   /web/libweb.rmeta    the web crate's metadata (ADR 0024)

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { ConsoleStdout, Directory, File, type Inode, OpenFile, PreopenDirectory, WASI } from "@bjorn3/browser_wasi_shim";

import { buildWebCrate, sysrootDir, sysrootFiles, wasmPath } from "./site.ts";

const rustDir = join(import.meta.dir, "rust");

/** Compile ./rust/lib.rs, with `webCrate` as the web crate's metadata. Throws rustc's errors. */
export async function compileRust(webCrate: string) {
  const sources = new Map<string, Inode>(
    readdirSync(rustDir)
      .filter((name) => name.endsWith(".rs"))
      .map((name) => [name, new File(readFileSync(join(rustDir, name)))]),
  );
  const sysroot = new Map<string, Inode>(
    sysrootFiles().map((name) => [name, new File(readFileSync(join(sysrootDir, name)), { readonly: true })]),
  );
  const dir = (entries: Record<string, Inode>) => new Directory(new Map(Object.entries(entries)));
  const crate = new PreopenDirectory("/wasm/web/rust", sources);
  const stderr: string[] = [];
  const fds = [
    new OpenFile(new File([])), // stdin
    ConsoleStdout.lineBuffered((line) => stderr.push(line)), // stdout
    ConsoleStdout.lineBuffered((line) => stderr.push(line)), // stderr
    crate,
    new PreopenDirectory("/sysroot", new Map([["lib", dir({ rustlib: dir({ "wasm32-unknown-unknown": dir({ lib: new Directory(sysroot) }) }) })]])),
    new PreopenDirectory("/web", new Map([["libweb.rmeta", new File(readFileSync(webCrate), { readonly: true })]])),
  ];
  const args = [
    "rust-js", "/wasm/web/rust/lib.rs", "-o", "/wasm/web/rust/lib.js",
    "--", "--target", "wasm32-unknown-unknown", "--sysroot", "/sysroot", "--extern", "web=/web/libweb.rmeta",
  ];
  // RUSTC_ICE=0: don't name a crash-report file after the process id (WASI has none).
  // Without options, the shim logs every call it handles.
  const wasi = new WASI(args, ["RUSTC_ICE=0"], fds, { debug: false });
  const instance = await WebAssembly.instantiate(await WebAssembly.compile(readFileSync(wasmPath)), {
    wasi_snapshot_preview1: wasi.wasiImport,
  });
  let exit: number | string;
  try {
    exit = wasi.start(instance as { exports: { memory: WebAssembly.Memory; _start: () => unknown } });
  } catch (e) {
    // Errors end in a trap: panics can't unwind on wasm32-wasip1.
    exit = `trap (${e instanceof Error ? e.message : String(e)})`;
  }
  if (exit !== 0) throw new Error(`rust-js failed on wasm/web/rust/lib.rs (exit ${exit}):\n${stderr.join("\n")}`);
  // Warnings, on success.
  if (stderr.length > 0) console.warn(stderr.join("\n"));
  for (const [name, entry] of crate.dir.contents) {
    if (entry instanceof File && (name.endsWith(".js") || name.endsWith(".js.map"))) {
      writeFileSync(join(rustDir, name), entry.data);
    }
  }
}

// `bun compile-rust.ts` on its own.
if (import.meta.main) {
  const webCrate = join(import.meta.dir, "../target/web/libweb.rmeta");
  buildWebCrate(webCrate);
  await compileRust(webCrate);
  console.log("wrote rust/lib.js");
}
