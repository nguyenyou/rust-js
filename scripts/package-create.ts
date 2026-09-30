// Pack @rust-js/create (ADR 0105): its script, and as its template the
// vite-react example's files as git has them, with create/app's in
// place of the example's own. This never publishes to a registry.
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const [destination, ...extra] = Bun.argv.slice(2);
if (!destination || extra.length) throw new Error("Usage: bun scripts/package-create.ts <output.tgz>");
const output = resolve(destination);
const run = (args: string[], cwd = root) => {
  const result = Bun.spawnSync(args, { cwd, stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  return result.stdout.toString();
};
const version = (Bun.TOML.parse(readFileSync(join(root, "Cargo.toml"), "utf8")) as { package: { version: string } }).package.version;
const pkg = JSON.parse(readFileSync(join(root, "create/package.json"), "utf8"));
if (pkg.version !== version) throw new Error(`@rust-js/create ${pkg.version} does not match compiler ${version}`);
const staging = mkdtempSync(join(tmpdir(), "rust-js-create-"));
try {
  for (const file of ["index.js", "package.json", "README.md"]) copyFileSync(join(root, "create", file), join(staging, file));
  // What git has, so nothing building the example made goes with it.
  const example = "examples/vite-react";
  const app = "create/app";
  const overrides = new Set(readdirSync(join(root, app), { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => relative(join(root, app), join(entry.parentPath, entry.name))));
  for (const file of run(["git", "ls-files", example]).trim().split("\n")) {
    const path = relative(example, file);
    // npm leaves a package's `.gitignore` out: @rust-js/create names it back.
    const target = join(staging, "template", path === ".gitignore" ? "_gitignore" : path);
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(join(root, overrides.has(path) ? join(app, path) : file), target);
  }
  mkdirSync(dirname(output), { recursive: true });
  run([process.execPath, "pm", "pack", "--ignore-scripts", "--filename", output], staging);
  console.log(output);
} finally {
  rmSync(staging, { recursive: true, force: true });
}
