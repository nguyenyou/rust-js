// vite-plugin-rust-js: compile crates with rust-js when Vite starts and each
// time a `.rs` file is saved (ADR 0041). rust-js writes the JS beside the
// Rust, `src/App.rs` to `src/App.jsx`, and Vite serves that like any file of
// the project: plugin-react's Fast Refresh keeps the components' state.
//
//   save App.rs ─► rust-js ─► App.jsx changes ─► Vite: HMR update ─► Fast Refresh
//
// A compile error shows in Vite's overlay, and the page keeps the last JS
// that compiled.

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// This plugin lives in the rust-js repository, which has the compiler and the
// crates programs use: web (ADR 0024) and react (ADR 0041).
const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * @param {object} [options]
 * @param {string[]} [options.crates] Each crate's root file, from Vite's root.
 * @param {string} [options.rustJs] The rust-js binary.
 */
export default function rustJs({ crates = ["src/App.rs"], rustJs = join(repo, "target/debug/rust-js") } = {}) {
  const target = join(repo, "target");
  let root;
  let crateMetadata = false;

  // web's and react's metadata, which every crate is checked against.
  function buildCrates() {
    if (crateMetadata) return null;
    const p = spawnSync(join(repo, "react/build.sh"), ["-o", join(target, "libreact.rmeta")], { encoding: "utf8" });
    if (p.status !== 0) return `react/build.sh failed:\n${p.stderr || p.error?.message}`;
    crateMetadata = true;
    return null;
  }

  // Compile every crate. Paths are relative to Vite's root, which is what the
  // generated file's header and source map then show. Returns rustc's errors.
  function compile() {
    if (!existsSync(rustJs)) return `no rust-js at ${rustJs}: build it with \`bun run build\` in the rust-js repository`;
    const failed = buildCrates();
    if (failed) return failed;
    const errors = [];
    for (const crate of crates) {
      const output = crate.replace(/\.rs$/, ".js");
      const args = [crate, "-o", output, "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target];
      const p = spawnSync(rustJs, args, { cwd: root, encoding: "utf8" });
      if (p.status !== 0) errors.push(p.stderr || p.error?.message || `rust-js failed on ${crate}`);
    }
    return errors.length ? errors.join("\n") : null;
  }

  return {
    name: "rust-js",
    configResolved(config) {
      root = config.root;
    },
    // Before Vite reads any module: `vite build` stops on an error, the dev
    // server reports it and starts anyway.
    buildStart() {
      const errors = compile();
      if (errors && this.meta.watchMode) this.warn(errors);
      else if (errors) this.error(errors);
    },
    configureServer(server) {
      server.watcher.on("change", (file) => {
        if (!file.endsWith(".rs")) return;
        const errors = compile();
        if (!errors) return;
        server.config.logger.error(errors, { timestamp: true });
        server.ws.send({ type: "error", err: { message: errors, stack: "", plugin: "rust-js", id: file } });
      });
    },
  };
}
