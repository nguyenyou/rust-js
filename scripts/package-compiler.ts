// Create a local package for the current host; never publish or install globally.
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { parseCompilerIdentity } from "../tooling/manifest.js";

const root = resolve(import.meta.dir, "..");
const [compilerPath, destination, ...extra] = Bun.argv.slice(2);
if (!compilerPath || !destination || extra.length) throw new Error("Usage: bun scripts/package-compiler.ts <compiler> <output.tgz>");
if (!["darwin", "linux"].includes(process.platform)) throw new Error("Native packaging currently supports macOS and Linux hosts only");
const compiler = resolve(compilerPath);
const output = resolve(destination);
const run = (args: string[], cwd: string) => {
  const result = Bun.spawnSync(args, { cwd, stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  return result.stdout.toString();
};
const identity = parseCompilerIdentity(run([compiler, "--version-json"], root));
const version = (Bun.TOML.parse(readFileSync(join(root, "Cargo.toml"), "utf8")) as { package: { version: string } }).package.version;
const toolchain = (Bun.TOML.parse(readFileSync(join(root, "rust-toolchain.toml"), "utf8")) as { toolchain: { channel: string } }).toolchain.channel;
if (identity.version !== version || identity.toolchain !== toolchain) throw new Error("Compiler does not match this checkout's version and Rust pin");
// The native binary, not a launcher: one packaged as the compiler would start
// itself, again and again.
if (readFileSync(compiler).subarray(0, 2).toString() === "#!") {
  throw new Error(`${compiler} is a script, as an installed launcher is: package the native binary`);
}
const staging = mkdtempSync(join(tmpdir(), "rust-js-native-"));
try {
  mkdirSync(join(staging, "bin"));
  copyFileSync(compiler, join(staging, "bin/compiler"));
  copyFileSync(join(root, "tooling/native-launcher.js"), join(staging, "bin/rust-js"));
  chmodSync(join(staging, "bin/compiler"), 0o755);
  chmodSync(join(staging, "bin/rust-js"), 0o755);
  writeFileSync(join(staging, "package.json"), JSON.stringify({
    name: "@rust-js/native", version, type: "module", license: "MIT",
    repository: { type: "git", url: "git+https://github.com/rust-js-lang/rust-js.git" },
    os: [process.platform], cpu: [process.arch],
    bin: { "rust-js": "bin/rust-js" }, files: ["bin"], rustJs: identity,
  }, null, 2) + "\n");
  mkdirSync(dirname(output), { recursive: true });
  run([process.execPath, "pm", "pack", "--ignore-scripts", "--filename", output], staging);
  console.log(output);
} finally {
  rmSync(staging, { recursive: true, force: true });
}
