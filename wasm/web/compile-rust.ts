// Compile the playground's own Rust (./rust/lib.rs, and its components/) to JS, beside it, with
// rust-js.wasm: the compiler the page runs, here under the same WASI shim.
// vite.config.ts hands this to vite-plugin-rust-js, which calls it when Vite
// starts and on every save (ADR 0045).
//
//   /wasm/web/rust/...   the crate, and where lib.jsx and its maps go
//   /sysroot/...         the std metadata rustc type-checks against
//   /crates/...          the react crate's metadata (ADR 0044), and the web
//                        crate's it uses (ADR 0024)
//   /out/manifest.json   what it read and wrote (ADR 0042)

import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { ConsoleStdout, Directory, File, type Inode, OpenFile, PreopenDirectory, WASI } from "@bjorn3/browser_wasi_shim";

import { buildReactCrate, sysrootDir, sysrootFiles, wasmPath } from "./site.ts";

const rustDir = join(import.meta.dir, "rust");
const cratesDir = join(import.meta.dir, "../../target/playground-crates");
const virtual = "/wasm/web/rust";

let cratesBuilt = false;

/** The crate's `.rs` files under `dir`, its components/ too, as the shim's directories. */
function sourcesIn(dir: string): Map<string, Inode> {
  const entries = new Map<string, Inode>();
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) entries.set(entry.name, new Directory(sourcesIn(join(dir, entry.name))));
    else if (entry.name.endsWith(".rs")) entries.set(entry.name, new File(readFileSync(join(dir, entry.name))));
  }
  return entries;
}

/** Every file under a shim directory, by its path from there. */
function filesIn(dir: Directory, prefix = "", found = new Map<string, Uint8Array>()): Map<string, Uint8Array> {
  for (const [name, entry] of dir.contents) {
    if (entry instanceof Directory) filesIn(entry, `${prefix}${name}/`, found);
    else if (entry instanceof File) found.set(`${prefix}${name}`, entry.data);
  }
  return found;
}

/** The files under `dir` on disk, by their paths from there. */
function pathsIn(dir: string, prefix = ""): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? pathsIn(join(dir, entry.name), `${prefix}${entry.name}/`) : [`${prefix}${entry.name}`],
  );
}

/**
 * Compile ./rust/lib.rs, with the react and web crates. With `manifest`,
 * write the compiler's manifest there, its paths the real ones. Throws
 * rustc's errors.
 */
export async function compileRust(job: { manifest?: string } = {}) {
  if (!cratesBuilt) {
    buildReactCrate(cratesDir);
    cratesBuilt = true;
  }
  const crate = (name: string) => new File(readFileSync(join(cratesDir, name)), { readonly: true });
  const sources = sourcesIn(rustDir);
  const sysroot = new Map<string, Inode>(
    sysrootFiles().map((name) => [name, new File(readFileSync(join(sysrootDir, name)), { readonly: true })]),
  );
  const dir = (entries: Record<string, Inode>) => new Directory(new Map(Object.entries(entries)));
  const crateDir = new PreopenDirectory(virtual, sources);
  const out = new PreopenDirectory("/out", new Map());
  const stderr: string[] = [];
  const fds = [
    new OpenFile(new File([])), // stdin
    ConsoleStdout.lineBuffered((line) => stderr.push(line)), // stdout
    ConsoleStdout.lineBuffered((line) => stderr.push(line)), // stderr
    crateDir,
    new PreopenDirectory("/sysroot", new Map([["lib", dir({ rustlib: dir({ "wasm32-unknown-unknown": dir({ lib: new Directory(sysroot) }) }) })]])),
    new PreopenDirectory("/crates", new Map([["libreact.rmeta", crate("libreact.rmeta")], ["libweb.rmeta", crate("libweb.rmeta")]])),
    out,
  ];
  const args = [
    "rust-js", `${virtual}/lib.rs`, "-o", `${virtual}/lib.js`, "--manifest", "/out/manifest.json",
    "--", "--target", "wasm32-unknown-unknown", "--sysroot", "/sysroot",
    "--extern", "web=/crates/libweb.rmeta", "--extern", "react=/crates/libreact.rmeta", "-L", "/crates",
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

  // A module with JSX is a `.jsx` file (ADR 0040). A file whose content is
  // the same is left alone, so Vite doesn't update what didn't change; one an
  // earlier build wrote that this one didn't goes.
  const written = /\.jsx?(\.map)?$/;
  const outputs = new Map([...filesIn(crateDir.dir)].filter(([path]) => written.test(path)));
  for (const path of pathsIn(rustDir).filter((p) => written.test(p) && !outputs.has(p))) rmSync(join(rustDir, path));
  for (const [name, data] of outputs) {
    const path = join(rustDir, name);
    if (existsSync(path) && Buffer.from(readFileSync(path)).equals(Buffer.from(data))) continue;
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, data);
  }

  if (job.manifest) {
    const manifest = out.dir.contents.get("manifest.json");
    if (!(manifest instanceof File)) throw new Error("rust-js wrote no manifest");
    // The compiler saw the crate at /wasm/web/rust; it's at rustDir.
    const text = new TextDecoder().decode(manifest.data).replaceAll(`"${virtual}/`, `"${rustDir}/`);
    writeFileSync(job.manifest, text);
  }
}

// `bun compile-rust.ts` on its own.
if (import.meta.main) {
  await compileRust();
  console.log("wrote rust/**/*.jsx");
}
