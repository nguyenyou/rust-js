// Assemble a local distribution. No registry publication or global installation.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { parseCompilerIdentity } from "../tooling/manifest.js";

const root = resolve(import.meta.dir, "..");
const [compilerPath, destination, ...extra] = Bun.argv.slice(2);
if (!compilerPath || !destination || extra.length) throw new Error("Usage: bun scripts/package-distribution.ts <compiler> <new-output-directory>");
const compiler = resolve(compilerPath);
const output = resolve(destination);
if (existsSync(output)) throw new Error(`Output already exists: ${output}; choose a new distribution directory`);
const run = (args: string[], cwd = root) => {
  const result = Bun.spawnSync(args, { cwd, stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  return result.stdout.toString();
};
const identity = parseCompilerIdentity(run([compiler, "--version-json"]));
for (const directory of ["tooling", "vite-plugin", "runtime"]) {
  const pkg = JSON.parse(readFileSync(join(root, directory, "package.json"), "utf8"));
  if (pkg.version !== identity.version) throw new Error(`${pkg.name} ${pkg.version} does not match compiler ${identity.version}`);
}
// The commit it's made from, and whether the checkout had changes of its
// own, before anything is made in it, so what qualifies it can be run from
// the same one.
const source = { commit: run(["git", "rev-parse", "HEAD"]).trim(), clean: run(["git", "status", "--porcelain"]).trim() === "" };
mkdirSync(dirname(output), { recursive: true });
const staging = mkdtempSync(join(dirname(output), ".rust-js-distribution-"));
try {
  // @rust-js/runtime, what the JS the compiler writes imports (ADR 0103).
  for (const [directory, file] of [["tooling", "build.tgz"], ["vite-plugin", "vite-plugin.tgz"], ["runtime", "runtime.tgz"]]) {
    run([process.execPath, "pm", "pack", "--ignore-scripts", "--filename", join(staging, file)], join(root, directory));
  }
  run([process.execPath, "scripts/package-resources.ts", join(staging, "resources.tgz")]);
  run([process.execPath, "scripts/package-compiler.ts", compiler, join(staging, "native.tgz")]);
  const files = ["build.tgz", "vite-plugin.tgz", "runtime.tgz", "resources.tgz", "native.tgz"];
  const sha256 = (file: string) => createHash("sha256").update(readFileSync(join(staging, file))).digest("hex");
  const artifacts = files.map(file => ({ file, sha256: sha256(file) }));
  writeFileSync(join(staging, "distribution.json"), JSON.stringify({
    version: 1, compiler: identity, source, platform: process.platform, arch: process.arch, artifacts,
  }, null, 2) + "\n");
  writeFileSync(join(staging, "SHA256SUMS"), [...files, "distribution.json"].map(file => `${sha256(file)}  ${file}\n`).join(""));
  // The destination must be new; never replace a previous bundle on failure.
  if (existsSync(output)) throw new Error(`Output already exists: ${output}`);
  renameSync(staging, output);
  console.log(output);
} finally {
  rmSync(staging, { recursive: true, force: true });
}
