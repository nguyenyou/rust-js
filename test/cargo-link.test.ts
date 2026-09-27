import { beforeAll, expect, test } from "bun:test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCompiler, compiler, fixture, root, run } from "./support";
import { planCargoLibraries } from "../tooling/cargo.js";
import { parseManifest } from "../tooling/manifest.js";

beforeAll(buildCompiler, 600_000);
const pin = readFileSync(join(root, "rust-toolchain.toml"), "utf8").match(/channel = "([^"]+)"/)![1];
// What rust-js checks programs for (ADR 0090), so what their dependencies are built for.
const target = "wasm32-unknown-unknown";

test("a real Cargo dependency links scalar functions and function values into JS", async () => {
  const dir = fixture("cargo-shared-library");
  writeFileSync(join(dir, "Cargo.toml"), '[workspace]\nmembers = ["shared", "app"]\nresolver = "2"\n');
  for (const name of ["shared", "app"]) {
    mkdirSync(join(dir, name, "src"), { recursive: true });
    writeFileSync(join(dir, name, "Cargo.toml"), `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2024"\n` + (name === "app" ? '[dependencies]\nmodel = { package = "shared", path = "../shared" }\n' : ''));
  }
  const source = join(dir, "shared/src/lib.rs");
  writeFileSync(source, 'pub fn increment(n: u32) -> u32 { n + 1 }\npub fn positive(n: i32) -> bool { n > 0 }\n');
  writeFileSync(join(dir, "app/src/lib.rs"), 'pub mod nested { pub mod inner { pub fn run() -> u32 { let increment = model::increment; if model::positive(1) { increment(40) + 1 } else { 0 } } } }\npub fn run() -> u32 { nested::inner::run() }\n');
  writeFileSync(join(dir, "app/src/main.rs"), 'fn main() { println!("{}", app::run()); }');
  const manifestPath = join(dir, "app/Cargo.toml");
  run(["cargo", `+${pin}`, "generate-lockfile", "--offline", "--manifest-path", manifestPath]);
  const graph = await planCargoLibraries({ manifestPath, toolchain: pin, target });
  expect(graph.libraries.map(p => p.name)).toEqual(["shared", "app"]);
  const libraryManifest = join(dir, "js/shared/manifest.json");
  const output = join(dir, "js/app/lib.js"), manifest = join(dir, "js/app/manifest.json");
  function compile() {
    const messages = run(["cargo", `+${pin}`, "check", "--frozen", "--lib", "--target", target, "--manifest-path", manifestPath, "--message-format=json"])
      .trim().split("\n").map(line => JSON.parse(line));
    const shared = messages.find(m => m.reason === "compiler-artifact" && m.target.name === "shared");
    const metadata = shared.filenames.find((p: string) => p.endsWith(".rmeta"));
    run([compiler, source, "-o", join(dir, "js/shared/lib.js"), "--library", "--manifest", libraryManifest, "--", "--crate-name", "shared"]);
    return [compiler, join(dir, "app/src/lib.rs"), "-o", output, "--manifest", manifest, "--dependency", libraryManifest, "--", "--crate-name", "app", "--extern", `model=${metadata}`, "-L", `dependency=${join(dir, "target", target, "debug/deps")}`];
  }
  let command = compile();
  run(command);
  const native = () => Number(run(["cargo", `+${pin}`, "run", "--frozen", "--quiet", "--manifest-path", manifestPath]));
  const javascript = () => Number(run([process.execPath, "--eval", `const m = await import(${JSON.stringify(output)}); console.log(m.run());`]));
  expect(javascript()).toBe(native());
  expect(javascript()).toBe(42);
  const generated = readFileSync(join(dir, "js/app/nested/inner.js"), "utf8");
  expect(generated).toContain('../../shared/lib.js');
  const consumer = parseManifest(readFileSync(manifest, "utf8"));
  expect(consumer.sources).toContain(libraryManifest);
  expect(consumer.sources).toContain(source);
  writeFileSync(source, 'pub fn increment(n: u32) -> u32 { n + 2 }\npub fn positive(n: i32) -> bool { n > 0 }\n');
  expect(() => run(command)).toThrow("dependency artifact changed");
  command = compile();
  run(command);
  expect(javascript()).toBe(native());
  expect(javascript()).toBe(43);
  const previous = readFileSync(output, "utf8");
  const original = readFileSync(libraryManifest, "utf8");
  parseManifest(original);
  for (const [edit, message] of [
    [(m: any) => m.library.version = 999, "unsupported library ABI"],
    [(m: any) => m.compiler.toolchain = "wrong", "incompatible dependency compiler identity"],
    [(m: any) => m.library.functions[0].signature.output = "bool", "dependency signature mismatch"],
    [(m: any) => m.library.functions = [], "does not export"],
  ] as const) {
    const contract = JSON.parse(original);
    edit(contract);
    writeFileSync(libraryManifest, JSON.stringify(contract));
    expect(() => run(command)).toThrow(message);
    expect(readFileSync(output, "utf8")).toBe(previous);
  }
  writeFileSync(libraryManifest, original);
  writeFileSync(join(dir, "js/shared/lib.js"), "export function increment() { return 999; }");
  expect(() => run(command)).toThrow("dependency artifact changed");
  expect(readFileSync(output, "utf8")).toBe(previous);
}, 120_000);

test("scalar linkage supports sibling outputs and rejects generic and aggregate APIs", () => {
  const dir = fixture("scalar-library");
  const source = join(dir, "shared.rs"), metadata = join(dir, "libshared.rmeta");
  writeFileSync(source, `
    pub fn notify() {}
    pub fn negate(value: bool) -> bool { !value }
    pub fn generic<T>(value: T) -> T { value }
    pub fn pair() -> (u32, u32) { (1, 2) }
  `);
  run(["rustc", `+${pin}`, source, "--crate-name", "shared", "--crate-type=lib", "--emit=metadata", `--target=${target}`, "-o", metadata]);
  const libraryManifest = join(dir, "shared.json");
  run([compiler, source, "-o", join(dir, ".shared.js"), "--library", "--manifest", libraryManifest]);
  const app = join(dir, "app.rs"), output = join(dir, "app.js");
  const command = [compiler, app, "-o", output, "--dependency", libraryManifest, "--", "--extern", `shared=${metadata}`];
  writeFileSync(app, "pub fn run() -> bool { shared::notify(); shared::negate(false) }");
  run(command);
  expect(readFileSync(output, "utf8")).toContain('from "./.shared.js"');
  expect(run([process.execPath, "--eval", `const m = await import(${JSON.stringify(output)}); console.log(m.run());`]).trim()).toBe("true");
  const previous = readFileSync(output, "utf8");
  for (const expression of ["shared::generic(1u32)", "shared::pair().0"]) {
    writeFileSync(app, `pub fn run() -> u32 { ${expression} }`);
    expect(() => run(command)).toThrow("does not export");
    expect(readFileSync(output, "utf8")).toBe(previous);
  }
}, 120_000);
