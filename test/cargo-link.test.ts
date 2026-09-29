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

// A library is compiled by rust-js, which writes its JS, its manifest, and
// the metadata its consumers' rustc reads, from one build (ADR 0100).
test("a real Cargo dependency links functions and function values into JS", async () => {
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
  const metadata = join(dir, "js/shared/libshared.rmeta");
  const output = join(dir, "js/app/lib.js"), manifest = join(dir, "js/app/manifest.json");
  const library = () => run([compiler, source, "-o", join(dir, "js/shared/lib.js"), "--library", "--manifest", libraryManifest, "--", "--crate-name", "shared", `--emit=metadata=${metadata}`]);
  // Cargo's alias for it, `model`, doesn't change the crate it is.
  const command = [compiler, join(dir, "app/src/lib.rs"), "-o", output, "--manifest", manifest, "--dependency", libraryManifest, "--", "--crate-name", "app", "--extern", `model=${metadata}`];
  library();
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
  library();
  run(command);
  expect(javascript()).toBe(native());
  expect(javascript()).toBe(43);
  const previous = readFileSync(output, "utf8");
  const original = readFileSync(libraryManifest, "utf8");
  parseManifest(original);
  for (const [edit, message] of [
    [(m: any) => m.library.version = 999, "unsupported library ABI"],
    [(m: any) => m.compiler.toolchain = "wrong", "incompatible dependency compiler identity"],
    [(m: any) => m.library.items = [], "isn't one of the items `shared`'s manifest exports"],
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

test("a library's generic and aggregate functions link too, from a sibling output", () => {
  const dir = fixture("sibling-library");
  const source = join(dir, "shared.rs"), metadata = join(dir, "libshared.rmeta");
  writeFileSync(source, `
    pub fn notify() {}
    pub fn negate(value: bool) -> bool { !value }
    pub fn generic<T>(value: T) -> T { value }
    pub fn pair() -> (u32, u32) { (1, 2) }
  `);
  const libraryManifest = join(dir, "shared.json");
  run([compiler, source, "-o", join(dir, ".shared.js"), "--library", "--manifest", libraryManifest, "--", "--crate-name", "shared", `--emit=metadata=${metadata}`]);
  const app = join(dir, "app.rs"), output = join(dir, "app.js");
  const command = [compiler, app, "-o", output, "--dependency", libraryManifest, "--", "--extern", `shared=${metadata}`];
  const result = () => run([process.execPath, "--eval", `const m = await import(${JSON.stringify(output)}); console.log(m.run());`]).trim();
  writeFileSync(app, "pub fn run() -> bool { shared::notify(); shared::negate(false) }");
  run(command);
  expect(readFileSync(output, "utf8")).toContain('from "./.shared.js"');
  expect(result()).toBe("true");
  writeFileSync(app, "pub fn run() -> u32 { shared::generic(1u32) + shared::pair().1 }");
  run(command);
  expect(result()).toBe("3");
}, 120_000);
