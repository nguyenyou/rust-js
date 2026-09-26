// Compile Rust to ordinary JS/JSX files; Vite and plugin-react own bundling
// and Fast Refresh. The compiler manifest owns dependencies and output paths.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { mkdir, readFile, stat } from "node:fs/promises";
import { dirname, extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const target = join(repo, "target");
const metadataInputs = [
  "rust-toolchain.toml", "react/build.sh", "react/cfg.ts", "react/versions.json",
  "react/src/lib.rs", "react/src/event.rs", "react/src/dom.rs", "react/src/elements.rs",
  "web/build.sh", "web/src/lib.rs",
].map(p => join(repo, p));

function run(command, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, stdio: ["ignore", "ignore", "pipe"] });
    let errors = "";
    child.stderr.setEncoding("utf8").on("data", chunk => { errors += chunk; });
    child.on("error", reject);
    child.on("close", code => code === 0 ? resolve() : reject(new Error(errors || `${command} exited with ${code}`)));
  });
}

/**
 * @param {object} [options]
 * @param {string[]} [options.crates] Crate roots relative to Vite's root.
 * @param {string} [options.rustJs] Compiler binary; defaults to this checkout.
 * @param {(job: { crate: string, output: string, manifest: string }) => Promise<void>} [options.compile]
 *   Compile a crate some other way, like the playground's with rust-js.wasm:
 *   write its JS beside it and a manifest (ADR 0042) to `manifest`, all paths
 *   absolute, or throw rustc's errors. It builds the crates it needs itself.
 */
export default function rustJs({ crates = ["src/App.rs"], rustJs = join(target, "debug/rust-js"), compile: custom } = {}) {
  let root, server, closed = false, metadataKey;
  let active;
  const pending = new Set();
  const manifests = new Map();
  const failures = new Map();
  const aliases = new Map();
  const maps = new Set();
  // Committed files used without rust-js, whose maps may not be committed.
  const committed = new Set();
  const manifestPath = crate => join(target, "vite", createHash("sha256").update(resolve(root, crate)).digest("hex") + ".json");

  // The React the project has installed, whose API the react crate is built
  // with (ADR 0043): what a later React added doesn't compile. `null` without
  // one, which gets the latest's.
  function installedReact() {
    try {
      const require = createRequire(join(root, "package.json"));
      return JSON.parse(readFileSync(require.resolve("react/package.json"), "utf8")).version;
    } catch {
      return null;
    }
  }

  // web's and react's metadata for that React, each version in its own folder.
  let metadata, react;
  async function buildMetadata() {
    if (!existsSync(rustJs)) throw new Error(`no rust-js at ${rustJs}: run bun run build in the rust-js repository`);
    react = installedReact();
    metadata = join(target, "react", react ?? "latest");
    const compiler = await stat(rustJs);
    const hash = createHash("sha256").update(`${compiler.mtimeMs}:${compiler.size}:${react}`);
    for (const path of metadataInputs) hash.update(await readFile(path));
    const key = hash.digest("hex");
    if (metadataKey === key && existsSync(join(metadata, "libreact.rmeta")) && existsSync(join(metadata, "libweb.rmeta"))) return;
    await mkdir(metadata, { recursive: true });
    await run(join(repo, "react/build.sh"), ["-o", join(metadata, "libreact.rmeta"), ...(react ? ["--react", react] : [])], repo);
    metadataKey = key;
  }

  async function compile(crate) {
    const manifest = manifestPath(crate);
    await mkdir(dirname(manifest), { recursive: true });
    try {
      const output = crate.replace(/\.rs$/, ".js");
      if (custom) await custom({ crate: resolve(root, crate), output: resolve(root, output), manifest });
      else await run(rustJs, [crate, "-o", output, "--manifest", manifest,
        "--", "--extern", `react=${join(metadata, "libreact.rmeta")}`, "-L", metadata], root);
    } catch (error) {
      // rustc names the version an item needs; say which one is installed.
      if (react && error.message.includes("configured out")) {
        error.message += `\nnote: this project has React ${react}; an item gated \`react = "X.Y"\` needs React X.Y or later\n`;
      }
      throw error;
    }
    const result = JSON.parse(await readFile(manifest, "utf8"));
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
      try {
        if (!custom) await buildMetadata();
        for (const crate of batch) {
          try { await compile(crate); }
          catch (error) { failures.set(crate, error.message); }
        }
      } catch (error) {
        for (const crate of batch) failures.set(crate, error.message);
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
    const common = file === resolve(rustJs) || metadataInputs.includes(file);
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
      rustJs = resolve(root, rustJs);
    },
    async buildStart() {
      // The generated JS is committed, as ReScript recommends (ADR 0041), so
      // a checkout without rust-js still builds from it. With rust-js, the
      // Rust is always compiled, and an error is never hidden by an old file.
      if (!custom && !existsSync(rustJs)) {
        const files = crates.map(crate => [".jsx", ".js"].map(ext => crate.replace(/\.rs$/, ext)).find(file => existsSync(resolve(root, file))));
        if (files.every(Boolean)) {
          for (const file of files) committed.add(resolve(root, file));
          this.warn(`no rust-js at ${rustJs}: using the committed ${files.join(", ")}. Build rust-js (bun run build) to compile the Rust.`);
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
    resolveId(source, importer) {
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
      server.watcher.add([...metadataInputs, rustJs, ...crates.map(crate => dirname(resolve(root, crate)))]);
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
