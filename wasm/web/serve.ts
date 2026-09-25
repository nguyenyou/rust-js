// Dev server for the playground page: rust-js.wasm running in the browser.
//
//   /                          the page (Bun bundles main.ts and its imports)
//   /rust-js.wasm              the WASI build from ../build.sh
//   /sysroot.json              names of the metadata files rustc needs
//   /sysroot/<name>            those files
//   /examples.json             the example crates: names, roots, file lists
//   /examples/<name>/<path>    their files
//
// build.ts produces the same layout as static files, for GitHub Pages.

import { join } from "node:path";

import page from "./index.html";
import { examples, examplesManifest, sysrootDir, sysrootFiles, wasmPath } from "./site.ts";

const sysroot = sysrootFiles();
const notFound = () => new Response("not found", { status: 404 });

const server = Bun.serve({
  port: Number(process.env.PORT ?? 4400),
  routes: {
    "/": page,
    "/rust-js.wasm": () => new Response(Bun.file(wasmPath), { headers: { "Content-Type": "application/wasm" } }),
    "/sysroot.json": Response.json(sysroot),
    "/sysroot/:name": (req) =>
      sysroot.includes(req.params.name) ? new Response(Bun.file(join(sysrootDir, req.params.name))) : notFound(),
    "/examples.json": () => Response.json(examplesManifest()),
    "/examples/*": (req) => {
      // Only files an example lists: never an arbitrary path.
      const [name, ...rest] = new URL(req.url).pathname.slice("/examples/".length).split("/");
      const example = examples().find((e) => e.name === name);
      const file = rest.join("/");
      return example?.files.includes(file) ? new Response(Bun.file(join(example.dir, file))) : notFound();
    },
  },
});

console.log(`rust-js in the browser: ${server.url}`);
