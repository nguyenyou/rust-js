import { expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, chmodSync, realpathSync, readFileSync, readdirSync, existsSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildCompiler, compiler, root as repository } from "./support";

const node = Bun.which("node");
if (!node) throw new Error("Node.js is required for distribution runtime compatibility tests");

for (const runtime of [process.execPath, node]) {
test(`installed packages compile using ${runtime} without the other runtime`, () => {
  buildCompiler();
  const root = realpathSync(mkdtempSync(join(tmpdir(), "rust-js packages ")));
  const blocked = join(root, "blocked-runtime");
  mkdirSync(blocked);
  const other = join(blocked, runtime === node ? "bun" : "node");
  writeFileSync(other, "#!/bin/sh\necho 'Unexpected dependency on the other JS runtime' >&2\nexit 99\n");
  chmodSync(other, 0o755);
  const run = (args: string[], cwd = root) => {
    const result = Bun.spawnSync(args, { cwd, stdout: "pipe", stderr: "pipe", env: { ...process.env, PATH: `${blocked}:${process.env.PATH}` } });
    if (result.exitCode !== 0) throw new Error(result.stderr.toString());
    return result.stdout.toString();
  };
  try {
    const bundle = join(root, "artifacts");
    const pack = [process.execPath, join(repository, "scripts/package-distribution.ts"), compiler, bundle];
    run(pack);
    const distribution = JSON.parse(readFileSync(join(bundle, "distribution.json"), "utf8"));
    expect(distribution.compiler).toEqual(JSON.parse(run([compiler, "--version-json"])));
    expect(distribution.artifacts).toHaveLength(4);
    run(["shasum", "-a", "256", "-c", "SHA256SUMS"], bundle);
    const previousDistribution = readFileSync(join(bundle, "distribution.json"), "utf8");
    expect(() => run(pack)).toThrow("Output already exists");
    expect(readFileSync(join(bundle, "distribution.json"), "utf8")).toBe(previousDistribution);
    const incompatible = join(root, "incompatible-compiler");
    writeFileSync(incompatible, `#!/bin/sh\ncat <<'IDENTITY'\n${JSON.stringify({ ...distribution.compiler, toolchain: "nightly-2000-01-01" })}\nIDENTITY\n`);
    chmodSync(incompatible, 0o755);
    const failedBundle = join(root, "failed-artifacts");
    expect(() => run([process.execPath, join(repository, "scripts/package-distribution.ts"), incompatible, failedBundle])).toThrow("Compiler does not match");
    expect(existsSync(failedBundle)).toBe(false);
    expect(readdirSync(root).some(name => name.startsWith(".rust-js-distribution-"))).toBe(false);
    const resources = join(root, "node_modules/rust-js-resources");
    writeFileSync(join(root, "package.json"), JSON.stringify({
      private: true, type: "module", dependencies: {
        "rust-js-build": "./artifacts/rust-js-build.tgz",
        "vite-plugin-rust-js": "./artifacts/vite-plugin-rust-js.tgz",
        "rust-js-resources": "./artifacts/resources.tgz",
        "rust-js-native": "./artifacts/native.tgz",
      },
      overrides: { "rust-js-build": "./artifacts/rust-js-build.tgz" },
    }));
    // No registry access or lifecycle scripts. Real Vite is exercised by
    // vite.test.ts; this test invokes its plugin hooks without the peer.
    const install = [process.execPath, "install", "--offline", "--ignore-scripts", "--omit", "peer", "--backend", "copyfile"];
    run(install);
    run([...install, "--frozen-lockfile"]);
    const plugin = JSON.parse(readFileSync(join(root, "node_modules/vite-plugin-rust-js/package.json"), "utf8"));
    expect(plugin.dependencies["rust-js-build"]).toBe("0.1.0");
    const installedCompiler = join(root, "node_modules/.bin/rust-js");
    expect(JSON.parse(run([runtime, installedCompiler, "--version-json"]))).toEqual(JSON.parse(run([compiler, "--version-json"])));
    const resourcePackage = JSON.parse(readFileSync(join(resources, "package.json"), "utf8"));
    expect(resourcePackage.version).toBe(Bun.TOML.parse(readFileSync(join(repository, "Cargo.toml"), "utf8")).package.version);
    expect(readFileSync(join(resources, "rust-toolchain.toml"), "utf8")).toBe(readFileSync(join(repository, "rust-toolchain.toml"), "utf8"));
    writeFileSync(join(root, "lib.rs"), `
#[derive(serde::Deserialize)]
pub struct Message { pub count: u32 }
pub fn answer() -> u32 {
    let message: Message = serde_json::from_str(r#"{"count":42}"#).unwrap();
    message.count
}
`);
    writeFileSync(join(root, "App.rs"), `
#![allow(non_snake_case)]
use react::Element;
pub fn App() -> Element { jsx! { <main><span>{"Packaged"}</span></main> } }
`);
    writeFileSync(join(root, "check.js"), `
import rustJs from "vite-plugin-rust-js";
import { parseManifest } from "rust-js-build/manifest";
import { publishArtifacts } from "rust-js-build/publish";
import { createNativeBuilder } from "rust-js-build/build";
import { readFileSync, readdirSync } from "node:fs";
const compiler = ${JSON.stringify(installedCompiler)};
const inputs = createNativeBuilder({ root: ${JSON.stringify(root)} }).watchFiles;
const symlinkInputs = createNativeBuilder({ root: ${JSON.stringify(root)}, rustJs: compiler }).watchFiles;
const plugin = rustJs({ crates: ["lib.rs", "App.rs"], bindings: ["react", "serde"], cacheDir: ${JSON.stringify(join(root, "cache"))} });
plugin.configResolved({ root: ${JSON.stringify(root)} });
const watched = [];
await plugin.buildStart.call({ addWatchFile: file => watched.push(file), warn: message => { throw new Error(message); }, error: message => { throw new Error(message); } });
const manifest = readdirSync("cache/vite").map(file => parseManifest(readFileSync("cache/vite/" + file, "utf8"))).find(value => value.input.endsWith("/lib.rs"));
const { answer } = await import("./lib.js");
console.log(JSON.stringify({ answer: answer(), watched, inputs, symlinkInputs, input: manifest.input, publisher: typeof publishArtifacts }));
`);
    const result = JSON.parse(run([runtime, "check.js"]));
    expect(result.answer).toBe(42);
    expect(result.watched).toContain(join(root, "lib.rs"));
    expect(result.input).toBe(join(root, "lib.rs"));
    expect(result.publisher).toBe("function");
    expect(result.inputs).toContain(join(root, "node_modules/rust-js-native/bin/compiler"));
    expect(result.symlinkInputs).toContain(join(root, "node_modules/rust-js-native/bin/compiler"));
    const jsx = readFileSync(join(root, "App.jsx"), "utf8");
    expect(jsx).toContain("<main>");
    expect(jsx).toContain("<span>Packaged</span>");
    const previous = readFileSync(join(root, "lib.js"), "utf8");
    const resourceManifest = join(resources, "package.json");
    writeFileSync(resourceManifest, JSON.stringify({ ...resourcePackage, version: "999.0.0" }));
    expect(() => run([runtime, "check.js"])).toThrow("Incompatible rust-js resources");
    expect(readFileSync(join(root, "lib.js"), "utf8")).toBe(previous);
    writeFileSync(join(bundle, "resources.tgz"), "damaged archive");
    expect(() => run(["shasum", "-a", "256", "-c", "SHA256SUMS"], bundle)).toThrow();
    writeFileSync(resourceManifest, JSON.stringify(resourcePackage));
    const pinPath = join(resources, "rust-toolchain.toml");
    const originalPin = readFileSync(pinPath, "utf8");
    writeFileSync(pinPath, originalPin.replace(/channel\s*=\s*"[^"]+"/, 'channel = "nightly-2000-01-01"'));
    expect(() => run([runtime, "check.js"])).toThrow("Install matching compiler and resources");
    expect(readFileSync(join(root, "lib.js"), "utf8")).toBe(previous);
    writeFileSync(pinPath, originalPin);
    expect(JSON.parse(run([runtime, "check.js"])).answer).toBe(42);
    writeFileSync(join(root, "lib.rs"), "pub fn broken(");
    expect(() => run([runtime, "check.js"])).toThrow("unclosed delimiter");
    expect(readFileSync(join(root, "lib.js"), "utf8")).toBe(previous);
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 600_000);
}
