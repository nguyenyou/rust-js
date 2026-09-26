import { mkdirSync, mkdtempSync } from "node:fs";
import { join } from "node:path";

export const root = join(import.meta.dir, "..");
export const target = join(root, "target");
export const compiler = join(target, "debug", "rust-js");

export function run(cmd: string[]): string {
  const p = Bun.spawnSync(cmd, { cwd: root, stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`${cmd.join(" ")} failed:\n${p.stderr.toString()}`);
  return p.stdout.toString();
}

let built = false;
export function buildCompiler() {
  if (!built) {
    run(["cargo", "build", "--quiet"]);
    built = true;
  }
}
let web = false;
export function buildWeb() {
  if (!web) {
    buildCompiler();
    run(["web/build.sh", "-o", join(target, "libweb.rmeta")]);
    web = true;
  }
}
export function fixture(name: string): string {
  mkdirSync(target, { recursive: true });
  return mkdtempSync(join(target, `${name}-`));
}

let react = false;
export function buildReact() {
  if (!react) {
    buildCompiler();
    run(["react/build.sh", "-o", join(target, "libreact.rmeta")]);
    react = true;
  }
}
