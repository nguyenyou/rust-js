// Build the playground as static files in ./dist, for GitHub Pages:
//
//   dist/index.html + bundled JS
//   dist/rust-js.wasm
//   dist/sysroot.json, dist/sysroot/*.rmeta
//   dist/fib.rs
//
// Every URL the page uses is relative, so it works under any base path
// (on Pages: https://<user>.github.io/rust-js/).

import { copyFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { examplePath, sysrootDir, sysrootFiles, wasmPath } from "./site.ts";

const dist = join(import.meta.dir, "dist");
rmSync(dist, { recursive: true, force: true });

const result = await Bun.build({
  entrypoints: [join(import.meta.dir, "index.html")],
  outdir: dist,
  minify: true,
  publicPath: "./",
});
if (!result.success) {
  for (const log of result.logs) console.error(log);
  process.exit(1);
}

const sysroot = sysrootFiles();
mkdirSync(join(dist, "sysroot"));
for (const name of sysroot) copyFileSync(join(sysrootDir, name), join(dist, "sysroot", name));
writeFileSync(join(dist, "sysroot.json"), JSON.stringify(sysroot));
copyFileSync(wasmPath, join(dist, "rust-js.wasm"));
copyFileSync(examplePath, join(dist, "fib.rs"));

console.log(`built ${dist}`);
