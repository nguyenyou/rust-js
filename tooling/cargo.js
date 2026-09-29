// Cargo's builds of rust-js crates (ADR 0101), and the experimental
// local-library planning before them (ADR 0085). Cargo owns resolution; this
// adapter does not infer dependencies from source files or run build scripts.
import { execFile } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { promisify } from "node:util";

import { fingerprint } from "./publish.js";

const execute = promisify(execFile);

/** @param {{ manifestPath: string, toolchain: string, target: string, packageName?: string, features?: string[], noDefaultFeatures?: boolean }} options */
export async function planCargoLibraries({ manifestPath, toolchain, target, packageName, features = [], noDefaultFeatures = false }) {
  if (!/^nightly-\d{4}-\d{2}-\d{2}$/.test(toolchain ?? "")) throw new Error("Cargo planning requires an exact nightly toolchain pin");
  if (typeof target !== "string" || !target) throw new Error("Cargo planning requires an explicit target triple");
  const manifest = resolve(manifestPath);
  const args = [`+${toolchain}`, "metadata", "--format-version=1", "--frozen", "--manifest-path", manifest, "--filter-platform", target];
  if (features.length) args.push("--features", features.join(","));
  if (noDefaultFeatures) args.push("--no-default-features");
  const { stdout } = await execute("cargo", args, { cwd: dirname(manifest), maxBuffer: 64 * 1024 * 1024 });
  const metadata = JSON.parse(stdout);
  const packages = new Map(metadata.packages.map(pkg => [pkg.id, pkg]));
  const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
  const selected = packageName
    ? metadata.workspace_members.filter(id => packages.get(id).name === packageName)
    : metadata.resolve.root ? [metadata.resolve.root] : [];
  if (selected.length !== 1) throw new Error("Select one Cargo workspace library with packageName");
  const ordered = [];
  const visited = new Set();
  const visiting = new Set();
  const pending = [{ id: selected[0] }];
  while (pending.length) {
    const { id, library: completed } = pending.pop();
    if (completed) {
      visiting.delete(id);
      visited.add(id);
      ordered.push(completed);
      continue;
    }
    if (visited.has(id)) continue;
    if (visiting.has(id)) throw new Error(`Cyclic Cargo library dependency: ${id}`);
    const pkg = packages.get(id), node = nodes.get(id);
    if (!pkg || !node) throw new Error(`Cargo metadata is missing package ${id}`);
    if (pkg.source !== null) throw new Error(`Cargo JS planning currently supports only local path libraries: ${pkg.name}`);
    if (pkg.targets.some(t => t.kind.includes("custom-build"))) throw new Error(`Cargo JS planning does not support build scripts: ${pkg.name}`);
    if (pkg.targets.some(t => t.kind.includes("proc-macro"))) throw new Error(`Cargo JS planning does not support procedural macros: ${pkg.name}`);
    const libraries = pkg.targets.filter(t => t.kind.includes("lib"));
    if (libraries.length !== 1) throw new Error(`Cargo JS planning requires one ordinary library target: ${pkg.name}`);
    const library = libraries[0];
    const dependencies = node.deps.filter(dep => dep.dep_kinds.some(kind => kind.kind === null))
      .map(dep => ({ name: dep.name, packageId: dep.pkg })).sort((a, b) => a.name.localeCompare(b.name));
    visiting.add(id);
    pending.push({ id, library: {
      id, name: pkg.name, crateName: library.name, edition: library.edition,
      manifestPath: pkg.manifest_path, sourcePath: library.src_path,
      features: [...node.features].sort(), dependencies,
    } });
    for (const dependency of [...dependencies].reverse()) pending.push({ id: dependency.packageId });
  }
  return { root: selected[0], toolchain, target, workspaceRoot: metadata.workspace_root, libraries: ordered };
}

/**
 * `cargo check` of a workspace for rust-js's target, with rust-js as Cargo's
 * workspace wrapper (ADR 0101), and where each crate rust-js compiled has its
 * JS: beside the metadata Cargo keeps for that build of it, so a feature set
 * built before is the JS it was. Cargo reports each crate it built or found
 * fresh; the `.rust-js` beside its metadata says where its manifest is. `js` is
 * the selected package's, or the manifest's own package's. `react` is the
 * React release the react crate is checked for (ADR 0043), or its latest.
 * With `inSource`, each module's JS is written beside its Rust too, as a
 * project commits it (`writeInSource`), and `js` and `crates` are those.
 * `files` is every module's JS, where it's served from.
 * @param {{ manifestPath: string, toolchain: string, compiler: string, packageName?: string, features?: string[], noDefaultFeatures?: boolean, offline?: boolean, react?: string, inSource?: boolean }} options
 * @returns {Promise<{ js: string, crates: Map<string, { js: string, manifest: string }>, files: string[] }>}
 */
export async function checkCargo({ manifestPath, toolchain, compiler, packageName, features = [], noDefaultFeatures = false, offline = false, react, inSource = false }) {
  if (!/^nightly-\d{4}-\d{2}-\d{2}$/.test(toolchain ?? "")) throw new Error("Cargo builds require an exact nightly toolchain pin");
  const manifest = resolve(manifestPath);
  const args = [`+${toolchain}`, "check", "--message-format=json", "--target", "wasm32-unknown-unknown", "--manifest-path", manifest];
  if (packageName) args.push("-p", packageName);
  if (features.length) args.push("--features", features.join(","));
  if (noDefaultFeatures) args.push("--no-default-features");
  if (offline) args.push("--offline");
  const env = { ...process.env, RUSTC_WORKSPACE_WRAPPER: resolve(compiler) };
  if (react) env.RUST_JS_REACT = react;
  else delete env.RUST_JS_REACT;
  const { stdout } = await execute("cargo", args, { cwd: dirname(manifest), env, maxBuffer: 64 * 1024 * 1024 }).catch((error) => {
    const messages = String(error.stdout ?? "").split("\n").filter(Boolean).map(line => JSON.parse(line));
    const rendered = messages.filter(m => m.reason === "compiler-message").map(m => m.message.rendered).join("");
    throw new Error(`cargo check failed:\n${rendered}${error.stderr ?? ""}`);
  });
  const crates = new Map();
  let js;
  for (const line of stdout.split("\n").filter(Boolean)) {
    const message = JSON.parse(line);
    if (message.reason !== "compiler-artifact") continue;
    const metadata = message.filenames.find(file => file.endsWith(".rmeta"));
    const marker = metadata?.replace(/\.rmeta$/, ".rust-js");
    if (!marker || !existsSync(marker)) continue;
    // Its own manifest, then those of the libraries it was compiled with.
    const library = readFileSync(marker, "utf8").split("\n")[0];
    const built = existsSync(library) ? JSON.parse(readFileSync(library, "utf8")) : { artifacts: [{ file: library }] };
    // What it published, as it published it, for a build Cargo has as done
    // too: Cargo checks its own outputs, not rust-js's.
    const changed = built.artifacts.find(({ file, hash }) => !existsSync(file) || fingerprint(readFileSync(file)) !== hash);
    if (changed) {
      const name = packageNameOf(message.package_id);
      throw new Error(`${changed.file} isn't what rust-js wrote for Cargo's build of ${name}, which has it as done: `
        + `\`cargo clean -p ${name} --target wasm32-unknown-unknown\` to build it again`);
    }
    // The crate root's module: `lib.jsx` when it has JSX (ADR 0075).
    const entry = { js: built.modules.find(module => module.module.length === 0).file, manifest: library };
    crates.set(message.target.name, entry);
    if (packageName ? packageNameOf(message.package_id) === packageName : message.manifest_path === manifest) js = entry.js;
  }
  if (!js) throw new Error(`rust-js compiled no library of ${packageName ?? manifest}`);
  const manifests = [...crates.values()].map(({ manifest }) => JSON.parse(readFileSync(manifest, "utf8")));
  if (!inSource) return { js, crates, files: manifests.flatMap(({ modules }) => modules.map((module) => module.file)) };
  const moved = writeInSource(manifests);
  for (const entry of crates.values()) entry.js = moved.get(entry.js);
  return { js: moved.get(js), crates, files: [...moved.values()] };
}

/** What rust-js writes first in a module's JS, and nowhere else. */
const GENERATED = "// Generated by rust-js from ";

/**
 * Each module's JS, and its map, beside its Rust, as ReScript writes it in
 * source and a project commits it (ADR 0041): `src/api.rs`'s is
 * `src/api.js`, or `.jsx`. A module that isn't a file of its own, an
 * inline `mod inner { .. }`, is where its file would be: `src/inner.js`
 * of `src/lib.rs`'s, `src/api/inner.js` of `src/api.rs`'s, and it's an
 * error if that's another module's, before anything's written. An import of another crate's module is of its
 * copy, where it is; any other stays as written. A file is written only if
 * it changed, and a module's JS rust-js wrote before, a `.js` or `.jsx`
 * beside a crate's Rust that's no module's now, goes. Where each JS went.
 * @param {{ library: { name: string }, modules: { file: string, map?: string, source: string, module: string[] }[] }[]} manifests
 * @returns {Map<string, string>}
 */
export function writeInSource(manifests) {
  const moved = new Map();
  // `mod root { .. }` of a `[lib] path = "src/root.rs"` is where the root
  // is, and two crates' inline `mod helper` of `sources/alpha.rs` and
  // `sources/beta.rs` are in one place: none is written over another, as
  // rust-js's own output refuses `mod lib` of `lib.rs`.
  const byDestination = new Map();
  const named = ({ crate, module }, other) => {
    const own = module.length === 0 ? "the crate root" : `module \`${module.join("::")}\``;
    if (crate === other.crate) return own;
    return `crate \`${crate}\`'s ${module.length === 0 ? "root" : own}`;
  };
  for (const { library, modules } of manifests) {
    const crateRoot = modules.find((m) => m.module.length === 0).source;
    for (const { file, source, module } of modules) {
      const extension = file.endsWith(".jsx") ? ".jsx" : ".js";
      // The module whose file `source` is, by where it is: `src/api.rs`
      // and `src/api/mod.rs` are `api`'s.
      const own = source === crateRoot ? [] : relative(dirname(crateRoot), source).replace(/\.rs$/, "").split(sep);
      if (own.at(-1) === "mod") own.pop();
      const written = own.length === module.length && own.every((name, i) => name === module[i]);
      moved.set(file, written ? source.replace(/\.rs$/, extension) : join(dirname(crateRoot), ...module) + extension);
    }
    for (const { file, module } of [...modules].sort((a, b) => a.module.length - b.module.length)) {
      const to = moved.get(file);
      const here = { crate: library.name, module };
      const other = byDestination.get(to);
      if (other) throw new Error(`rust-js can't write the JS in source: ${named(other, here)} and ${named(here, other)} would both be ${to}; rename the module`);
      byDestination.set(to, here);
    }
  }
  const write = (path, text) => {
    if (existsSync(path) && readFileSync(path, "utf8") === text) return;
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, text);
  };
  for (const { modules } of manifests) {
    for (const { file, map } of modules) {
      const to = moved.get(file);
      const text = readFileSync(file, "utf8").replace(/((?:from|import)\s*\(?\s*)"(\.{1,2}\/[^"]+)"/g, (whole, lead, spec) => {
        const target = moved.get(resolve(dirname(file), spec));
        if (!target) return whole;
        const path = relative(dirname(to), target);
        return `${lead}"${path.startsWith(".") ? path : `./${path}`}"`;
      });
      write(to, text);
      if (map && existsSync(map)) {
        const json = JSON.parse(readFileSync(map, "utf8"));
        json.sources = (json.sources ?? []).map((path) => relative(dirname(to), resolve(dirname(map), path)));
        write(`${to}.map`, JSON.stringify(json));
      }
    }
  }
  // What rust-js wrote before beside a crate's Rust, and isn't a module's now.
  const written = new Set(moved.values());
  for (const { modules } of manifests) {
    const root = dirname(modules.find((module) => module.module.length === 0).source);
    for (const name of readdirSync(root, { recursive: true }).map(String)) {
      const path = join(root, name);
      if (!/\.jsx?$/.test(path) || written.has(path)) continue;
      if (!readFileSync(path, "utf8").startsWith(GENERATED)) continue;
      rmSync(path);
      rmSync(`${path}.map`, { force: true });
    }
  }
  return moved;
}

/**
 * The workspace a Cargo manifest is of, a member's or the root's, and where
 * Cargo builds it: what Cargo reads, and what it writes (ADR 0101), and where each of its
 * packages is.
 * @param {{ manifestPath: string, toolchain: string, offline?: boolean }} options
 * @returns {Promise<{ root: string, target: string, packages: Map<string, string> }>}
 */
export async function cargoWorkspace({ manifestPath, toolchain, offline = false }) {
  const manifest = resolve(manifestPath);
  const args = [`+${toolchain}`, "metadata", "--no-deps", "--format-version=1", "--manifest-path", manifest, ...(offline ? ["--offline"] : [])];
  const { stdout } = await execute("cargo", args, { cwd: dirname(manifest), maxBuffer: 64 * 1024 * 1024 }).catch((error) => {
    throw new Error(`cargo metadata failed:\n${error.stderr ?? error.message}`);
  });
  const metadata = JSON.parse(stdout);
  const packages = new Map(metadata.packages.map((pkg) => [pkg.name, dirname(pkg.manifest_path)]));
  return { root: metadata.workspace_root, target: metadata.target_directory, packages };
}

/** The package's name in a Cargo package ID: `path+file:///dir#name@1.0`, or `path+file:///dir/name#1.0`. */
function packageNameOf(id) {
  const [url, fragment = ""] = id.split("#");
  return fragment.includes("@") ? fragment.slice(0, fragment.lastIndexOf("@")) : url.slice(url.lastIndexOf("/") + 1);
}
