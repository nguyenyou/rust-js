import { expect, test } from "bun:test";
import { mkdtempSync, realpathSync, writeFileSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseManifest, mapManifestPaths } from "../tooling/manifest.js";
import { createNativeBuilder } from "../tooling/build.js";
import { buildCompiler, compiler } from "./support";

const manifest = {
  version: 1, input: "/virtual/lib.rs", output: "/virtual/lib.js",
  sources: ["/virtual/lib.rs"],
  modules: [{ module: [], file: "/virtual/lib.js", map: "/virtual/lib.js.map", source: "/virtual/lib.rs", imports: [] }],
  artifacts: [{ file: "/virtual/lib.js", hash: "1234567890abcdef" }, { file: "/virtual/lib.js.map", hash: "1234567890abcdef" }],
};

test("hosts reject incompatible and malformed manifests before consuming paths", () => {
  for (const value of [null, { ...manifest, version: 2 }, { ...manifest, sources: [123] },
    { ...manifest, output: "relative.js" }, { ...manifest, artifacts: [] },
    { ...manifest, modules: [{ ...manifest.modules[0], imports: ["/missing.js"] }] }]) {
    expect(() => parseManifest(JSON.stringify(value))).toThrow();
  }
  expect(parseManifest(JSON.stringify(manifest))).toEqual(manifest);
});

test("virtual path mapping preserves JSON escaping and unrelated values", () => {
  const directory = '/local/a"quoted\\folder';
  const value = { ...manifest, note: "/virtual/not-a-path-field" };
  const mapped = mapManifestPaths(parseManifest(JSON.stringify(value)), path => path.replace("/virtual", directory));
  const reread = parseManifest(JSON.stringify(mapped));
  expect(reread.input).toBe(`${directory}/lib.rs`);
  expect(reread.modules[0].map).toBe(`${directory}/lib.js.map`);
  expect(reread.artifacts[0].hash).toBe(manifest.artifacts[0].hash);
  expect(reread.note).toBe(value.note);
});

test("native build adapter compiles an independent application and preserves output on failure", async () => {
  buildCompiler();
  const root = realpathSync(mkdtempSync(join(tmpdir(), "rust-js-independent-")));
  try {
    const source = join(root, "lib.rs");
    const output = join(root, "lib.js");
    const manifestPath = join(root, "manifest.json");
    writeFileSync(source, "pub fn answer() -> u32 { 42 }");
    const builder = createNativeBuilder({ root, rustJs: compiler, bindings: [] });
    await builder.compile({ crate: source, output, manifest: manifestPath });
    const result = parseManifest(readFileSync(manifestPath, "utf8"));
    expect(result.sources).toContain(source);
    expect((await import(output)).answer()).toBe(42);
    const previous = readFileSync(output, "utf8");
    writeFileSync(source, "pub fn broken(");
    await expect(builder.compile({ crate: source, output, manifest: manifestPath })).rejects.toThrow();
    expect(readFileSync(output, "utf8")).toBe(previous);
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 600_000);
