// A build publishes the same way whichever host runs it: the native
// compiler (src/output.rs) and the WASI host's publisher (tooling/publish.js)
// remove a file an older build wrote only if it's still theirs to remove
// (ADR 0091). Each case here runs against both, from the same older manifest.
//
//   older manifest: artifacts [lib.js, X]  ──►  this build writes lib.js
//                                          ──►  X removed, or kept?

import { beforeAll, expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, symlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { fingerprint, publishArtifacts } from "../tooling/publish.js";
import { buildCompiler, compiler, fixture } from "./support";

beforeAll(buildCompiler, 600_000);

type Case = {
  name: string;
  /** The older build's artifact, relative to the case's folder. */
  file: string;
  kept: boolean;
  /** Whether this build reads it, as `include_str!` does. */
  read?: boolean;
  /** A symlink to what this build reads, where the artifact was. */
  alias?: boolean;
  edited?: boolean;
};

const cases: Case[] = [
  { name: "an artifact this build doesn't write", file: "out/old.js", kept: false },
  { name: "an older map", file: "out/old.js.map", kept: false },
  { name: "one this build reads", file: "out/helper.js", read: true, kept: true },
  { name: "a path to what this build reads", file: "out/alias.js", read: true, alias: true, kept: true },
  { name: "one outside the output's directory", file: "elsewhere/old.js", kept: true },
  { name: "one that isn't JS or a map", file: "out/notes.txt", kept: true },
  { name: "one changed since it was written", file: "out/edited.js", edited: true, kept: true },
];

/** The folder for a case, its input and output, and the older manifest. */
function setUp(c: Case, host: string) {
  const dir = fixture(`publication-${host}`);
  mkdirSync(join(dir, "out"), { recursive: true });
  mkdirSync(join(dir, "elsewhere"), { recursive: true });
  const input = join(dir, "lib.rs");
  const output = join(dir, "out", "lib.js");
  const manifest = join(dir, "out", "lib.manifest.json");
  const file = join(dir, c.file);
  const helper = join(dir, "out", "helper.js");
  writeFileSync(helper, "export const helper = 1;\n");
  if (c.alias) symlinkSync(helper, file);
  else if (c.file !== "out/helper.js") writeFileSync(file, `written by an older build: ${c.name}\n`);
  const text = readFileSync(file);
  const read = c.read ? "pub const HELPER: &str = include_str!(\"out/helper.js\");\n" : "";
  writeFileSync(input, `${read}pub fn answer() -> i32 { 42 }\n`);
  const older = {
    version: 1,
    input,
    output,
    sources: [input],
    modules: [],
    artifacts: [{ file, hash: fingerprint(text) }],
  };
  writeFileSync(manifest, JSON.stringify(older, null, 2));
  if (c.edited) writeFileSync(file, "a person's edit\n");
  return { dir, input, output, manifest, file, helper };
}

for (const c of cases) {
  test(`native: ${c.name} is ${c.kept ? "kept" : "removed"}`, () => {
    const { input, output, manifest, file } = setUp(c, "native");
    const p = Bun.spawnSync([compiler, input, "-o", output, "--manifest", manifest], { stderr: "pipe" });
    expect(p.stderr.toString()).toBe("");
    expect(existsSync(file)).toBe(c.kept);
  });

  test(`WASI host: ${c.name} is ${c.kept ? "kept" : "removed"}`, () => {
    const { input, output, manifest, file, helper } = setUp(c, "host");
    const lib = Buffer.from("export function answer() { return 42; }\n");
    const next = {
      version: 1,
      input,
      output,
      sources: c.read ? [input, helper] : [input],
      modules: [],
      artifacts: [{ file: output, hash: fingerprint(lib) }],
    };
    publishArtifacts(manifest, next, new Map([[output, lib]]));
    expect(existsSync(file)).toBe(c.kept);
  });
}
