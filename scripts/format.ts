// rustfmt owns ordinary Rust; rust-js aligns the JSX that rustfmt leaves opaque.
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const args = Bun.argv.slice(2);
const check = args.includes("--check");
const files = args.filter(arg => arg !== "--check");
if (files.some(file => file.startsWith("-"))) {
  console.error("Usage: bun run fmt [--check] [file.rs ...]");
  process.exit(1);
}

function run(command: string[], input?: string): string {
  const result = Bun.spawnSync(command, {
    cwd: root,
    stdin: input === undefined ? "ignore" : Buffer.from(input),
    stdout: "pipe",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) {
    throw new Error(`${command.join(" ")}\n${result.stdout.toString()}${result.stderr.toString()}`);
  }
  return result.stdout.toString();
}

try {
  if (files.length === 0) {
    run(["cargo", "fmt", "--all", ...(check ? ["--check"] : [])]);
    files.push(...run([
      "git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "wasm/web/rust/*.rs",
    ]).split("\0").filter(Boolean));
  }
  run(["cargo", "build", "--locked", "--quiet"]);
  const changed: [string, string][] = [];
  for (const file of [...new Set(files)].sort()) {
    const path = resolve(root, file);
    const source = readFileSync(path, "utf8");
    const rust = run([
      "rustfmt", "--edition", "2024", "--config-path", root,
      "--config", "skip_children=true", "--emit", "stdout",
    ], source);
    const formatted = run([resolve(root, "target/debug/rust-js"), "--format-jsx"], rust);
    if (formatted !== source) changed.push([path, formatted]);
  }
  // Validate every input before writing any playground or explicitly selected file.
  for (const [path, formatted] of changed) {
    if (check) console.error(`Needs formatting: ${path}`);
    else {
      writeFileSync(path, formatted);
      console.log(`Formatted: ${path}`);
    }
  }
  if (check && changed.length > 0) process.exitCode = 1;
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
