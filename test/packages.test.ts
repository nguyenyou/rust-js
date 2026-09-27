import { expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, realpathSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildCompiler, compiler, root as repository } from "./support";

test("packed hosts and binding resources compile outside the repository layout", () => {
  buildCompiler();
  const root = realpathSync(mkdtempSync(join(tmpdir(), "rust-js packages ")));
  const run = (args: string[], cwd = root) => {
    const result = Bun.spawnSync(args, { cwd, stdout: "pipe", stderr: "pipe" });
    if (result.exitCode !== 0) throw new Error(result.stderr.toString());
    return result.stdout.toString();
  };
  try {
    for (const [directory, name] of [["tooling", "rust-js-build"], ["vite-plugin", "vite-plugin-rust-js"]]) {
      const archive = join(root, `${name}.tgz`);
      run([process.execPath, "pm", "pack", "--ignore-scripts", "--filename", archive], join(repository, directory));
      const destination = join(root, "node_modules", name);
      mkdirSync(destination, { recursive: true });
      run(["tar", "-xzf", archive, "--strip-components=1", "-C", destination]);
    }
    const plugin = JSON.parse(readFileSync(join(root, "node_modules/vite-plugin-rust-js/package.json"), "utf8"));
    expect(plugin.dependencies["rust-js-build"]).toBe("0.1.0");
    const resources = join(root, "node_modules/rust-js-resources");
    const resourceArchive = join(root, "resources.tgz");
    run([process.execPath, join(repository, "scripts/package-resources.ts"), resourceArchive]);
    mkdirSync(resources, { recursive: true });
    run(["tar", "-xzf", resourceArchive, "--strip-components=1", "-C", resources]);
    const resourcePackage = JSON.parse(readFileSync(join(resources, "package.json"), "utf8"));
    expect(resourcePackage.version).toBe(Bun.TOML.parse(readFileSync(join(repository, "Cargo.toml"), "utf8")).package.version);
    expect(readFileSync(join(resources, "rust-toolchain.toml"), "utf8")).toBe(readFileSync(join(repository, "rust-toolchain.toml"), "utf8"));
    writeFileSync(join(root, "package.json"), JSON.stringify({ private: true, type: "module" }));
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
    writeFileSync(join(root, "check.ts"), `
import rustJs from "vite-plugin-rust-js";
import { parseManifest } from "rust-js-build/manifest";
import { publishArtifacts } from "rust-js-build/publish";
import { readFileSync, readdirSync } from "node:fs";
const plugin = rustJs({ crates: ["lib.rs", "App.rs"], rustJs: ${JSON.stringify(compiler)}, resources: ${JSON.stringify(resources)}, bindings: ["react", "serde"], cacheDir: ${JSON.stringify(join(root, "cache"))} });
plugin.configResolved({ root: ${JSON.stringify(root)} });
const watched = [];
await plugin.buildStart.call({ addWatchFile: file => watched.push(file), warn: message => { throw new Error(message); }, error: message => { throw new Error(message); } });
const manifest = readdirSync("cache/vite").map(file => parseManifest(readFileSync("cache/vite/" + file, "utf8"))).find(value => value.input.endsWith("/lib.rs"));
const { answer } = await import("./lib.js");
console.log(JSON.stringify({ answer: answer(), watched, input: manifest.input, publisher: typeof publishArtifacts }));
`);
    const result = JSON.parse(run([process.execPath, "check.ts"]));
    expect(result.answer).toBe(42);
    expect(result.watched).toContain(join(root, "lib.rs"));
    expect(result.input).toBe(join(root, "lib.rs"));
    expect(result.publisher).toBe("function");
    const jsx = readFileSync(join(root, "App.jsx"), "utf8");
    expect(jsx).toContain("<main>");
    expect(jsx).toContain("<span>Packaged</span>");
    const previous = readFileSync(join(root, "lib.js"), "utf8");
    const resourceManifest = join(resources, "package.json");
    writeFileSync(resourceManifest, JSON.stringify({ ...resourcePackage, version: "999.0.0" }));
    expect(() => run([process.execPath, "check.ts"])).toThrow("Incompatible rust-js resources");
    expect(readFileSync(join(root, "lib.js"), "utf8")).toBe(previous);
    writeFileSync(resourceManifest, JSON.stringify(resourcePackage));
    const pinPath = join(resources, "rust-toolchain.toml");
    const originalPin = readFileSync(pinPath, "utf8");
    writeFileSync(pinPath, originalPin.replace(/channel\s*=\s*"[^"]+"/, 'channel = "nightly-2000-01-01"'));
    expect(() => run([process.execPath, "check.ts"])).toThrow("Install matching compiler and resources");
    expect(readFileSync(join(root, "lib.js"), "utf8")).toBe(previous);
    writeFileSync(pinPath, originalPin);
    expect(JSON.parse(run([process.execPath, "check.ts"])).answer).toBe(42);
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 600_000);
