// Build a local resource tarball. This never publishes to a registry.
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { bindingInputs, resourceInputs } from "../tooling/resources.js";

const root = resolve(import.meta.dir, "..");
const [destination, ...extra] = Bun.argv.slice(2);
if (!destination || extra.length) throw new Error("Usage: bun scripts/package-resources.ts <output.tgz>");
const output = resolve(destination);
const staging = mkdtempSync(join(tmpdir(), "rust-js-resources-"));
try {
  const version = Bun.TOML.parse(readFileSync(join(root, "Cargo.toml"), "utf8")).package.version;
  const files = resourceInputs(Object.keys(bindingInputs));
  for (const file of files) {
    const target = join(staging, file);
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(join(root, file), target);
  }
  writeFileSync(join(staging, "package.json"), JSON.stringify({
    name: "rust-js-resources", version, private: true, type: "module",
    description: "Pinned build inputs for rust-js React, web, and Serde bindings.",
    files,
  }, null, 2) + "\n");
  mkdirSync(dirname(output), { recursive: true });
  const result = Bun.spawnSync([process.execPath, "pm", "pack", "--ignore-scripts", "--filename", output], {
    cwd: staging, stdout: "pipe", stderr: "pipe",
  });
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  console.log(output);
} finally {
  rmSync(staging, { recursive: true, force: true });
}
