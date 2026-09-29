import { expect } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

import { runSync, stopped } from "./child";

export const root = join(import.meta.dir, "..");
export const target = join(root, "target");
// This checkout's debug build, or with `RUST_JS_COMPILER`, another, as the
// rustc tests workflow's build job makes one for its machines to share.
const given = process.env.RUST_JS_COMPILER;
export const compiler = given ? resolve(given) : join(target, "debug", "rust-js");

/** What `cmd` printed, with `env` added to its environment, or why it
 * failed. It's stopped after `timeout` ms:
 * as the tests load, no test's deadline applies yet. The default is what
 * a setup hook is allowed, for the first build. */
export function run(cmd: string[], timeout = 600_000, env: Record<string, string> = {}): string {
  const p = runSync(cmd, root, timeout, env);
  const why = stopped(p, timeout);
  if (p.code !== 0 || why) throw new Error(`${cmd.join(" ")} failed${why ? `: it ${why}` : ""}:\n${p.stderr}`);
  return p.stdout;
}

let built = false;
export function buildCompiler() {
  if (!built && !given) {
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

/** serde, serde_derive and serde_json (ADR 0077): the flags that find them,
 * as metadata for rust-js or as libraries for a native build. */
const serdeFlags: Record<string, string[]> = {};
export function buildSerde(kind: "rmeta" | "rlib" = "rmeta"): string[] {
  serdeFlags[kind] ??= run(["serde/build.sh", ...(kind === "rlib" ? ["--rlib"] : [])]).trim().split(/\s+/);
  return serdeFlags[kind];
}

let react = false;
export function buildReact() {
  if (!react) {
    buildCompiler();
    run(["react/build.sh", "-o", join(target, "libreact.rmeta")]);
    react = true;
  }
}

/** The generated JS files under `dir`, by their paths from there: no maps. */
function generated(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { recursive: true }).map(String).filter((path) => /\.jsx?$/.test(path)).sort();
}

/**
 * Compare the JS in `actual` with its snapshot in `snapshot`, file by file:
 * the same files, each with the same text. With `BLESS=1` (`bun run bless`),
 * write `actual` as the snapshot instead, unless `bless` is false: for a check
 * against a snapshot another compile made. `normalize` evens out what may
 * differ without meaning anything, like where the compiler saw the source.
 */
export function expectSnapshot(
  actual: string,
  snapshot: string,
  {
    normalize = (text: string) => text,
    hint = "the generated JS changed: if that's intended, run `bun run bless` and review the diff",
    bless = true,
  }: { normalize?: (text: string) => string; hint?: string; bless?: boolean } = {},
) {
  const files = generated(actual);
  if (bless && process.env.BLESS) {
    rmSync(snapshot, { recursive: true, force: true });
    for (const path of files) {
      mkdirSync(dirname(join(snapshot, path)), { recursive: true });
      writeFileSync(join(snapshot, path), normalize(readFileSync(join(actual, path), "utf8")));
    }
    return;
  }
  expect(files, hint).toEqual(generated(snapshot));
  for (const path of files) {
    const text = normalize(readFileSync(join(actual, path), "utf8"));
    expect(text, `${path}: ${hint}`).toBe(readFileSync(join(snapshot, path), "utf8"));
  }
}
