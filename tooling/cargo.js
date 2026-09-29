// Cargo's builds of rust-js crates (ADR 0101), and the experimental
// local-library planning before them (ADR 0085). Cargo owns resolution; this
// adapter does not infer dependencies from source files or run build scripts.
import { execFile } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
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
 * the selected package's, or the manifest's own package's.
 * @param {{ manifestPath: string, toolchain: string, compiler: string, packageName?: string, features?: string[], noDefaultFeatures?: boolean, offline?: boolean }} options
 * @returns {Promise<{ js: string, crates: Map<string, { js: string, manifest: string }> }>}
 */
export async function checkCargo({ manifestPath, toolchain, compiler, packageName, features = [], noDefaultFeatures = false, offline = false }) {
  if (!/^nightly-\d{4}-\d{2}-\d{2}$/.test(toolchain ?? "")) throw new Error("Cargo builds require an exact nightly toolchain pin");
  const manifest = resolve(manifestPath);
  const args = [`+${toolchain}`, "check", "--message-format=json", "--target", "wasm32-unknown-unknown", "--manifest-path", manifest];
  if (packageName) args.push("-p", packageName);
  if (features.length) args.push("--features", features.join(","));
  if (noDefaultFeatures) args.push("--no-default-features");
  if (offline) args.push("--offline");
  const env = { ...process.env, RUSTC_WORKSPACE_WRAPPER: resolve(compiler) };
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
    const entry = { js: built.output, manifest: library };
    crates.set(message.target.name, entry);
    if (packageName ? packageNameOf(message.package_id) === packageName : message.manifest_path === manifest) js = entry.js;
  }
  if (!js) throw new Error(`rust-js compiled no library of ${packageName ?? manifest}`);
  return { js, crates };
}

/** The package's name in a Cargo package ID: `path+file:///dir#name@1.0`, or `path+file:///dir/name#1.0`. */
function packageNameOf(id) {
  const [url, fragment = ""] = id.split("#");
  return fragment.includes("@") ? fragment.slice(0, fragment.lastIndexOf("@")) : url.slice(url.lastIndexOf("/") + 1);
}
