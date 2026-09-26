// Compile Rust to ordinary JS/JSX files; Vite and plugin-react own bundling
// and Fast Refresh. The compiler manifest owns dependencies and output paths.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readFile, stat } from "node:fs/promises";
import { dirname, extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const target = join(repo, "target");
const metadataInputs = ["rust-toolchain.toml", "react/build.sh", "react/src/lib.rs", "web/build.sh", "web/src/lib.rs"].map(p => join(repo, p));

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
 */
export default function rustJs({ crates = ["src/App.rs"], rustJs = join(target, "debug/rust-js") } = {}) {
  let root, server, closed = false, metadataKey;
  let active;
  const pending = new Set();
  const manifests = new Map();
  const failures = new Map();
  const aliases = new Map();
  const manifestPath = crate => join(target, "vite", createHash("sha256").update(resolve(root, crate)).digest("hex") + ".json");

  async function buildMetadata() {
    if (!existsSync(rustJs)) throw new Error(`no rust-js at ${rustJs}: run bun run build in the rust-js repository`);
    const compiler = await stat(rustJs);
    const hash = createHash("sha256").update(`${compiler.mtimeMs}:${compiler.size}`);
    for (const path of metadataInputs) hash.update(await readFile(path));
    const key = hash.digest("hex");
    if (metadataKey === key && existsSync(join(target, "libreact.rmeta")) && existsSync(join(target, "libweb.rmeta"))) return;
    await mkdir(target, { recursive: true });
    await run(join(repo, "react/build.sh"), ["-o", join(target, "libreact.rmeta")], repo);
    metadataKey = key;
  }

  async function compile(crate) {
    const manifest = manifestPath(crate);
    await mkdir(dirname(manifest), { recursive: true });
    await run(rustJs, [crate, "-o", crate.replace(/\.rs$/, ".js"), "--manifest", manifest,
      "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target], root);
    const result = JSON.parse(await readFile(manifest, "utf8"));
    const old = manifests.get(crate);
    manifests.set(crate, result);
    failures.delete(crate);
    server?.watcher.add(result.sources);
    aliases.clear();
    for (const current of manifests.values()) {
      for (const module of current.modules) {
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
        await buildMetadata();
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
      await schedule(crates);
      if (failures.size) {
        const message = [...failures.values()].join("\n");
        if (server) this.warn(message);
        else this.error(message);
      }
      for (const manifest of manifests.values()) for (const file of manifest.sources) this.addWatchFile(file);
    },
    resolveId(source, importer) {
      if (!importer || !source.startsWith(".")) return;
      return aliases.get(resolve(dirname(importer.split("?")[0]), source));
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
