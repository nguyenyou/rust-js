// Compile Rust to ordinary JS/JSX files; Vite and plugin-react own bundling
// and Fast Refresh. The compiler manifest owns dependencies and output paths.
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readFile } from "node:fs/promises";
import { dirname, extname, join, resolve, sep } from "node:path";
import { createNativeBuilder, findCompiler } from "rust-js-build/build";
import { parseManifest } from "rust-js-build/manifest";

/**
 * @param {object} [options]
 * @param {string[]} [options.crates] Crate roots relative to Vite's root.
 * @param {string} [options.rustJs] Compiler binary; defaults to the app's native package, then this checkout.
 * @param {string[]} [options.bindings] Built-in metadata recipes: react (default), serde.
 * @param {(job: { crate: string, output: string, manifest: string }) => Promise<void>} [options.compile]
 *   Compile a crate some other way, like the playground's with rust-js.wasm:
 *   write its JS beside it and a manifest (ADR 0042) to `manifest`, all paths
 *   absolute, or throw rustc's errors. It builds the crates it needs itself.
 * @param {{ package: string, manifestPath?: string, features?: string[], noDefaultFeatures?: boolean, offline?: boolean }} [options.cargo]
 *   Build a Cargo workspace instead (ADR 0101): Cargo checks each crate, with
 *   rust-js as its workspace wrapper, and the app imports `package`'s JS,
 *   where Cargo's build of it has it, as `rust-js:<package>`. `manifestPath`
 *   is relative to Vite's root, `Cargo.toml` by default.
 */
export default function rustJs({ crates = ["src/App.rs"], rustJs, compile: custom, resources, rustcFlags = [], cacheDir, bindings, externs, cargo } = {}) {
  // Cargo's checks are the native compiler's (ADR 0101), not another's.
  if (cargo && custom) throw new Error("rust-js: give `cargo` or `compile`, not both");
  if (cargo) crates = [cargo.package];
  let root, server, builder, closed = false;
  // A Cargo build's: the package's JS, the manifest Vite is given, and the
  // workspace it's of, what Cargo reads, and its target, what it writes.
  let entry, workspaceManifest, workspace, targetDir;
  let active;
  const pending = new Set();
  const manifests = new Map();
  const failures = new Map();
  const aliases = new Map();
  const maps = new Set();
  // The modules Cargo's builds have, outside the app.
  const generated = new Set();
  // Committed files used without rust-js, whose maps may not be committed.
  const committed = new Set();
  const manifestPath = crate => join(cacheDir ?? join(root, "node_modules/.cache/rust-js"), "vite", createHash("sha256").update(resolve(root, crate)).digest("hex") + ".json");

  // Every crate Cargo built, or found done: each is a manifest here, and
  // every edit is one `cargo check`, which rebuilds what it changed.
  async function compileCargo() {
    const old = JSON.stringify([...manifests.values()].flatMap(m => m.artifacts.map(a => a.file)).sort());
    // A member's manifest is of the workspace its siblings are of too.
    ({ root: workspace, target: targetDir } = await builder.cargoWorkspace({ manifestPath: workspaceManifest, offline: cargo.offline }));
    server?.watcher.add(workspace);
    const built = await builder.checkCargo({
      manifestPath: workspaceManifest, packageName: cargo.package,
      features: cargo.features, noDefaultFeatures: cargo.noDefaultFeatures, offline: cargo.offline,
    });
    manifests.clear();
    for (const [name, { manifest }] of built.crates) manifests.set(name, parseManifest(await readFile(manifest, "utf8")));
    entry = built.js;
    failures.clear();
    // Its imports name each module's file, `.jsx` or `.js`: no aliases.
    maps.clear();
    generated.clear();
    for (const current of manifests.values()) {
      server?.watcher.add([...current.sources, ...current.modules.map(module => module.file)]);
      for (const module of current.modules) {
        generated.add(module.file);
        if (module.map) maps.add(module.map);
      }
    }
    // Another build of a crate, of other features say, is another module ID.
    const now = JSON.stringify([...manifests.values()].flatMap(m => m.artifacts.map(a => a.file)).sort());
    if (server && old !== "[]" && old !== now) {
      server.moduleGraph.invalidateAll();
      server.ws.send({ type: "full-reload" });
    }
  }

  async function compile(crate) {
    if (cargo) return compileCargo();
    const manifest = manifestPath(crate);
    await mkdir(dirname(manifest), { recursive: true });
    const output = crate.replace(/\.rs$/, ".js");
    if (custom) await custom({ crate: resolve(root, crate), output: resolve(root, output), manifest });
    else await builder.compile({ crate, output, manifest });
    const result = parseManifest(await readFile(manifest, "utf8"));
    const old = manifests.get(crate);
    manifests.set(crate, result);
    failures.delete(crate);
    server?.watcher.add(result.sources);
    aliases.clear();
    maps.clear();
    for (const current of manifests.values()) {
      for (const module of current.modules) {
        if (module.map) maps.add(resolve(root, module.map));
        const file = resolve(root, module.file);
        const stem = file.slice(0, -extname(file).length);
        aliases.set(stem + ".js", file);
        aliases.set(stem + ".jsx", file);
      }
    }
    // A suffix or module-set change invalidates old Vite module IDs. Ordinary
    // edits retain their IDs and use plugin-react's state-preserving refresh.
    if (server && old && JSON.stringify(old.artifacts.map(a => a.file)) !== JSON.stringify(result.artifacts.map(a => a.file))) {
      server.moduleGraph.invalidateAll();
      server.ws.send({ type: "full-reload" });
    }
  }

  async function drain() {
    while (pending.size && !closed) {
      // Let filesystem events from a single save accumulate before spawning.
      await new Promise(resolve => setTimeout(resolve, 30));
      const batch = [...pending];
      pending.clear();
      for (const crate of batch) {
        try { await compile(crate); }
        catch (error) { failures.set(crate, error.message); }
      }
    }
    if (server && !closed) {
      if (failures.size) {
        const message = [...failures.values()].join("\n");
        server.config.logger.error(message, { timestamp: true });
        server.ws.send({ type: "error", err: { message, stack: "", plugin: "rust-js" } });
      } else {
        // Vite clears its error overlay on an update, even when correcting an
        // error reproduces identical JS and therefore triggers no file update.
        server.ws.send({ type: "update", updates: [] });
      }
    }
  }

  function schedule(affected) {
    for (const crate of affected) pending.add(crate);
    if (!active) active = drain().finally(() => { active = undefined; });
    return active;
  }
  function changed(event, path) {
    if (closed) return;
    const file = resolve(path);
    const common = builder?.watchFiles.includes(file);
    if (cargo) {
      // What Cargo reads of the workspace, and each source rust-js read,
      // wherever it is: a module by `#[path]` outside it. Cargo's own
      // outputs aren't.
      const read = /\.rs$|(^|[\\/])Cargo\.(toml|lock)$/.test(file) && file.startsWith(workspace + sep) && !(targetDir && file.startsWith(targetDir + sep));
      const source = [...manifests.values()].some(manifest => manifest.sources.includes(file));
      if (common || read || source) void schedule(crates);
      return;
    }
    if (!common && !file.endsWith(".rs")) return;
    const affected = crates.filter(crate => {
      if (common || manifests.get(crate)?.sources.includes(file) || resolve(root, crate) === file) return true;
      // Newly declared or missing modules aren't in the last successful
      // manifest. Retry roots under whose source tree the edit occurred.
      return failures.has(crate) || ((event === "add" || !manifests.has(crate)) && file.startsWith(dirname(resolve(root, crate)) + sep));
    });
    void schedule(affected);
  }

  return {
    name: "rust-js",
    enforce: "pre",
    configResolved(config) {
      root = config.root;
      if (cargo) {
        workspaceManifest = resolve(root, cargo.manifestPath ?? "Cargo.toml");
        workspace = dirname(workspaceManifest);
      }
      if (!custom) {
        rustJs = resolve(root, rustJs ?? findCompiler(root));
        builder = createNativeBuilder({ root, rustJs, resources, rustcFlags, cacheDir, bindings, externs });
      }
    },
    async buildStart() {
      // The generated JS is committed, as ReScript recommends (ADR 0041), so
      // a checkout without rust-js still builds from it. With rust-js, the
      // Rust is always compiled, and an error is never hidden by an old file.
      if (!custom && !cargo && !existsSync(rustJs)) {
        const files = crates.map(crate => [".jsx", ".js"].map(ext => crate.replace(/\.rs$/, ext)).find(file => existsSync(resolve(root, file))));
        if (files.every(Boolean)) {
          for (const file of files) committed.add(resolve(root, file));
          this.warn(`no rust-js at ${rustJs}: using the committed ${files.join(", ")}. Install rust-js or build it with cargo build to compile the Rust.`);
          return;
        }
      }
      await schedule(crates);
      if (failures.size) {
        const message = [...failures.values()].join("\n");
        if (server) this.warn(message);
        else this.error(message);
      }
      for (const manifest of manifests.values()) for (const file of manifest.sources) this.addWatchFile(file);
    },
    // A committed file names its source map, which isn't committed: without
    // it, drop the comment rather than have Vite report a missing file.
    async load(id) {
      const file = id.split("?")[0];
      if (!committed.has(file) || existsSync(`${file}.map`)) return;
      return (await readFile(file, "utf8")).replace(/\/\/# sourceMappingURL=\S+\s*$/, "");
    },
    resolveId(source, importer, options) {
      if (cargo && source === `rust-js:${cargo.package}`) return entry;
      // A package a module Cargo built imports is the app's, where the app
      // has it installed: resolved as from Vite's root, not the target's.
      if (importer && generated.has(importer.split("?")[0]) && !source.startsWith(".") && !source.startsWith("/")) {
        return this.resolve(source, join(root, "index.html"), { ...options, skipSelf: true });
      }
      if (!importer || !source.startsWith(".")) return;
      return aliases.get(resolve(dirname(importer.split("?")[0]), source));
    },
    // Neither a `.rs` file nor a source map written from one is a module: the
    // JS compiled from them brings its own update. Pass on only what depends
    // on a `.rs` file as a plain file, like a stylesheet holding its Tailwind
    // classes, which then updates in place. Otherwise Tailwind reloads the
    // page, as it does for a template file it scans.
    hotUpdate({ file, modules }) {
      if (maps.has(file)) return [];
      if (!file.endsWith(".rs")) return;
      return [...new Set(modules.flatMap(module => [...module.importers]))];
    },
    async handleHotUpdate() {
      // Native publication replaces files individually. Let Vite read the
      // completed module set, including its manifest, before sending updates.
      await active;
    },
    configureServer(value) {
      server = value;
      server.watcher.add([...(builder?.watchFiles ?? []), ...(cargo ? [workspace] : crates.map(crate => dirname(resolve(root, crate))))]);
      server.watcher.on("all", changed);
    },
    async closeBundle() {
      if (!server) return;
      closed = true;
      server?.watcher.off("all", changed);
      await active;
    },
    async closeWatcher() {
      closed = true;
      await active;
    },
  };
}
