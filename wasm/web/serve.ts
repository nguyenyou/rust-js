// Dev server for the playground page: rust-js.wasm running in the browser.
//
//   /                  the page (Bun bundles main.ts and its imports)
//   /rust-js.wasm      the WASI build from ../build.sh
//   /sysroot.json      names of the metadata files rustc needs
//   /sysroot/<name>    those files
//   /fib.rs            the example program, as a starting point
//
// build.ts produces the same layout as static files, for GitHub Pages.

import { join } from "node:path";

import page from "./index.html";
import { examplePath, sysrootDir, sysrootFiles, wasmPath } from "./site.ts";

const sysroot = sysrootFiles();

const server = Bun.serve({
  port: Number(process.env.PORT ?? 4400),
  routes: {
    "/": page,
    "/rust-js.wasm": () => new Response(Bun.file(wasmPath), { headers: { "Content-Type": "application/wasm" } }),
    "/sysroot.json": Response.json(sysroot),
    "/sysroot/:name": (req) =>
      sysroot.includes(req.params.name)
        ? new Response(Bun.file(join(sysrootDir, req.params.name)))
        : new Response("not found", { status: 404 }),
    "/fib.rs": () => new Response(Bun.file(examplePath)),
  },
});

console.log(`rust-js in the browser: ${server.url}`);
