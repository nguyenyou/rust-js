// Experimental local-library planning. Cargo owns resolution; this adapter does
// not infer dependencies from source files or run build scripts.
import { execFile } from "node:child_process";
import { dirname, resolve } from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);

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
