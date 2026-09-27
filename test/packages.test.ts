import { expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, realpathSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildCompiler, compiler, root as repository } from "./support";

test("packed host packages compile outside the repository layout", () => {
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
    writeFileSync(join(root, "package.json"), JSON.stringify({ private: true, type: "module" }));
    writeFileSync(join(root, "lib.rs"), "pub fn answer() -> u32 { 42 }");
    writeFileSync(join(root, "check.ts"), `
import rustJs from "vite-plugin-rust-js";
import { parseManifest } from "rust-js-build/manifest";
import { publishArtifacts } from "rust-js-build/publish";
import { readFileSync, readdirSync } from "node:fs";
const plugin = rustJs({ crates: ["lib.rs"], rustJs: ${JSON.stringify(compiler)}, bindings: [], cacheDir: ${JSON.stringify(join(root, "cache"))} });
plugin.configResolved({ root: ${JSON.stringify(root)} });
const watched = [];
await plugin.buildStart.call({ addWatchFile: file => watched.push(file), warn: message => { throw new Error(message); }, error: message => { throw new Error(message); } });
const manifest = parseManifest(readFileSync("cache/vite/" + readdirSync("cache/vite")[0], "utf8"));
const { answer } = await import("./lib.js");
console.log(JSON.stringify({ answer: answer(), watched, input: manifest.input, publisher: typeof publishArtifacts }));
`);
    const result = JSON.parse(run([process.execPath, "check.ts"]));
    expect(result.answer).toBe(42);
    expect(result.watched).toContain(join(root, "lib.rs"));
    expect(result.input).toBe(join(root, "lib.rs"));
    expect(result.publisher).toBe("function");
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 600_000);
