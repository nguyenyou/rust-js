import { expect, test } from "bun:test";
import { mkdtempSync, realpathSync, writeFileSync, rmSync, readFileSync, mkdirSync, copyFileSync, appendFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseManifest, mapManifestPaths, parseCompilerIdentity } from "../tooling/manifest.js";
import { createNativeBuilder } from "../tooling/build.js";
import { buildCompiler, compiler, root as repository } from "./support";

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

test("compiler identities reject incompatible ABI and missing version fields", () => {
  const identity = { version: "0.1.0", toolchain: "nightly-2026-03-25", abi: 1 };
  expect(parseCompilerIdentity(JSON.stringify(identity))).toEqual(identity);
  for (const value of [null, {}, [], { ...identity, abi: 2 }, { ...identity, version: "" }, { ...identity, toolchain: "" }]) {
    expect(() => parseCompilerIdentity(JSON.stringify(value))).toThrow();
    expect(() => parseManifest(JSON.stringify({ ...manifest, compiler: value }))).toThrow();
  }
});

test("scalar library manifests validate signatures and remap fingerprinted inputs", () => {
  const library = {
    version: 1, name: "shared", inputs: [{ file: "/virtual/shared.rs", hash: "1234567890abcdef" }],
    functions: [{ rust_path: "shared::answer", module: [], export: "answer", signature: { inputs: ["bool"], output: "u32" } }],
  };
  const value = { ...manifest, library };
  expect(parseManifest(JSON.stringify(value))).toEqual(value);
  const mapped = mapManifestPaths(value, path => path.replace("/virtual", "/local"));
  expect(mapped.library.inputs[0].file).toBe("/local/shared.rs");
  expect(mapped.library.functions).toEqual(library.functions);
  for (const invalid of [null, { ...library, version: 2 }, { ...library, inputs: [{ file: "relative.rs", hash: "bad" }] },
    { ...library, functions: [{ ...library.functions[0], signature: { inputs: ["unit"], output: "u32" } }] },
    { ...library, functions: [{ ...library.functions[0], signature: { inputs: [], output: "struct" } }] }]) {
    expect(() => parseManifest(JSON.stringify({ ...manifest, library: invalid }))).toThrow("library contract");
  }
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
    const version = Bun.spawnSync([compiler, "--version-json"], { stdout: "pipe", stderr: "pipe" });
    expect(version.exitCode).toBe(0);
    expect(parseCompilerIdentity(version.stdout.toString())).toEqual(result.compiler);
    expect(result.sources).toContain(source);
    expect((await import(output)).answer()).toBe(42);
    const previous = readFileSync(output, "utf8");
    writeFileSync(source, "pub fn broken(");
    await expect(builder.compile({ crate: source, output, manifest: manifestPath })).rejects.toThrow();
    expect(readFileSync(output, "utf8")).toBe(previous);
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 600_000);

test("build adapter prepares Serde for an independent app, reuses metadata, and rebuilds source edits", async () => {
  buildCompiler();
  const root = realpathSync(mkdtempSync(join(tmpdir(), "rust-js serde app ")));
  try {
    const source = join(root, "lib.rs");
    const output = join(root, "lib.js");
    const manifestPath = join(root, "manifest.json");
    const resources = join(root, "resources");
    mkdirSync(join(resources, "serde/src"), { recursive: true });
    for (const file of ["rust-toolchain.toml", "serde/Cargo.toml", "serde/Cargo.lock", "serde/src/lib.rs"]) {
      copyFileSync(join(repository, file), join(resources, file));
    }
    const options = { root, resources, rustJs: compiler, bindings: ["serde"], cacheDir: join(root, "cache with spaces") };
    const program = (offset: number) => `
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Message { pub count: u32 }
pub fn roundtrip(text: &str) -> String {
    let mut message: Message = serde_json::from_str(text).unwrap();
    message.count += ${offset};
    serde_json::to_string(&message).unwrap()
}`;
    writeFileSync(source, program(1));
    const builder = createNativeBuilder(options);
    const prepared = await builder.prepare();
    expect(prepared.flags.some(flag => flag.startsWith("serde="))).toBe(true);
    expect(builder.watchFiles.some(file => file.endsWith("serde/Cargo.lock"))).toBe(true);
    await builder.compile({ crate: source, output, manifest: manifestPath });
    expect((await import(output)).roundtrip('{"count":41}')).toBe('{"count":42}');
    // A new host process has no in-memory cache. Cargo must report the same artifacts.
    const reused = createNativeBuilder(options);
    expect((await reused.prepare()).flags).toEqual(prepared.flags);
    appendFileSync(join(resources, "serde/src/lib.rs"), "\n// Changed binding resource.\n");
    expect((await reused.prepare()).flags).not.toEqual(prepared.flags);
    writeFileSync(source, program(2));
    await reused.compile({ crate: source, output, manifest: manifestPath });
    expect((await import(output + "?updated")).roundtrip('{"count":41}')).toBe('{"count":43}');
    const previous = readFileSync(output, "utf8");
    writeFileSync(source, program(2).replace("message.count += 2", 'message.count += "bad"'));
    await expect(reused.compile({ crate: source, output, manifest: manifestPath })).rejects.toThrow();
    expect(readFileSync(output, "utf8")).toBe(previous);
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 600_000);
