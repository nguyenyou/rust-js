// Compile the playground's own Rust (./rust/lib.rs, and its components/) to JS, beside it, with
// rust-js.wasm: the compiler the page runs, here under the same WASI shim.
// vite.config.ts hands this to @rust-js/vite-plugin, which calls it when Vite
// starts and on every save (ADR 0045).
//
//   /wasm/web/rust/...   the crate, and where lib.jsx and its maps go
//   /sysroot/...         the std metadata rustc type-checks against
//   /crates/...          the react crate's metadata (ADR 0044), and the web
//                        crate's it uses (ADR 0024)
//   /out/manifest.json   what it read and wrote (ADR 0042)

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { ConsoleStdout, Directory, File, type Inode, OpenFile, PreopenDirectory, WASI } from "@bjorn3/browser_wasi_shim";

import { buildReactCrate, sysrootDir, sysrootFiles, wasmPath } from "./site.ts";
import { publishArtifacts } from "@rust-js/build/publish";
import { mapManifestPaths, parseManifest } from "@rust-js/build/manifest";

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

/**
 * Compile ./rust/lib.rs, with the react, webapi and js crates. With `manifest`,
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
    new PreopenDirectory("/crates", new Map(["libreact.rmeta", "libwebapi.rmeta", "libjs.rmeta"].map((name) => [name, crate(name)]))),
    out,
  ];
  const args = [
    "rust-js", `${virtual}/lib.rs`, "-o", `${virtual}/lib.js`, "--manifest", "/out/manifest.json",
    "--", "--target", "wasm32-unknown-unknown", "--sysroot", "/sysroot",
    "--extern", "webapi=/crates/libwebapi.rmeta", "--extern", "js=/crates/libjs.rmeta", "--extern", "react=/crates/libreact.rmeta", "-L", "/crates",
  ];
  // RUSTC_ICE=0: don't name a crash-report file after the process id (WASI has none).
  // RUSTC_BOOTSTRAP=1: the playground's own bindings declare JS types as extern
  // types, a nightly feature, until bindings have a stable way (ADR 0110); this
  // run compiles that one crate. Without options, the shim logs every call it handles.
  const wasi = new WASI(args, ["RUSTC_ICE=0", "RUSTC_BOOTSTRAP=1"], fds, { debug: false });
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

  const manifest = out.dir.contents.get("manifest.json");
  if (!(manifest instanceof File)) throw new Error("rust-js wrote no manifest");
  const parsed = parseManifest(new TextDecoder().decode(manifest.data));
  const localPath = (path: string) => {
    if (!path.startsWith(virtual + "/")) throw new Error("Unexpected virtual path: " + path);
    return join(rustDir, path.slice(virtual.length + 1));
  };
  const mapped = mapManifestPaths(parsed, localPath);
  const virtualFiles = filesIn(crateDir.dir);
  const outputs = new Map<string, Uint8Array>();
  for (const artifact of parsed.artifacts) {
    const data = virtualFiles.get(artifact.file.slice(virtual.length + 1));
    if (!data) throw new Error("Missing WASI artifact: " + artifact.file);
    outputs.set(localPath(artifact.file), data);
  }
  publishArtifacts(job.manifest ?? join(cratesDir, "playground-manifest.json"), mapped, outputs);
}

// `bun compile-rust.ts` on its own.
if (import.meta.main) {
  await compileRust();
  console.log("wrote rust/**/*.jsx");
}
