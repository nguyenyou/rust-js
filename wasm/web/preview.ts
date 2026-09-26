// Serve ./dist under /rust-js/, the way GitHub Pages will, to check the
// static build before deploying. Run `bun run site` first.

import { join, normalize } from "node:path";

const dist = join(import.meta.dir, "dist");
const base = "/rust-js/";

const server = Bun.serve({
  port: Number(process.env.PORT ?? 4401),
  async fetch(req) {
    const { pathname } = new URL(req.url);
    if (!pathname.startsWith(base)) return Response.redirect(base, 302);
    const rel = normalize(pathname.slice(base.length) || "index.html");
    if (rel.startsWith("..")) return new Response("not found", { status: 404 });
    const file = Bun.file(join(dist, rel));
    return (await file.exists()) ? new Response(file) : new Response("not found", { status: 404 });
  },
});

console.log(`static build at ${server.url.origin}${base}`);
