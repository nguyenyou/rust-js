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
  const version = (Bun.TOML.parse(readFileSync(join(root, "Cargo.toml"), "utf8")) as { package: { version: string } }).package.version;
  // An app's build needs the pin and its target, not this repository's
  // `rustc-dev` and tools, which rustup would install too: it installs what a
  // directory's `rust-toolchain.toml` lists before it runs `rustc` there,
  // as `react/build.sh` does (ADR 0105).
  const { channel, targets } = (Bun.TOML.parse(readFileSync(join(root, "rust-toolchain.toml"), "utf8")) as {
    toolchain: { channel: string; targets: string[] };
  }).toolchain;
  const toolchain = `# What an app's rust-js build runs (ADR 0105).\n[toolchain]\nchannel = ${JSON.stringify(channel)}\ntargets = ${JSON.stringify(targets)}\n`;
  const files = resourceInputs(Object.keys(bindingInputs));
  for (const file of files) {
    const target = join(staging, file);
    mkdirSync(dirname(target), { recursive: true });
    if (file === "rust-toolchain.toml") writeFileSync(target, toolchain);
    else copyFileSync(join(root, file), target);
  }
  writeFileSync(join(staging, "package.json"), JSON.stringify({
    name: "@rust-js/resources", version, type: "module", license: "MIT",
    repository: { type: "git", url: "git+https://github.com/rust-js-lang/rust-js.git" },
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
