#!/usr/bin/env node
// Packaged beside bin/compiler and package.json; no source-checkout paths.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";

const bin = dirname(fileURLToPath(import.meta.url));
const metadata = JSON.parse(readFileSync(join(bin, "../package.json"), "utf8"));
const { toolchain } = metadata.rustJs;
try {
  const result = spawnSync("rustc", [`+${toolchain}`, "--print", "sysroot"], { encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`rust-js runs with Rust ${toolchain}; install it once:\n  rustup toolchain install ${toolchain} --profile minimal --target wasm32-unknown-unknown\n${result.stderr}`);
  const sysroot = result.stdout.toString().trim();
  const key = process.platform === "darwin" ? "DYLD_LIBRARY_PATH" : "LD_LIBRARY_PATH";
  const libraryPath = [join(sysroot, "lib"), process.env[key]].filter(Boolean).join(":");
  const child = spawn(join(bin, "compiler"), process.argv.slice(2), {
    env: { ...process.env, [key]: libraryPath },
    stdio: "inherit",
  });
  child.on("error", error => { console.error(error.message); process.exitCode = 1; });
  child.on("exit", (code, signal) => {
    if (signal) process.kill(process.pid, signal);
    else process.exitCode = code;
  });
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
