import { expect, test } from "bun:test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { planCargoLibraries } from "../tooling/cargo.js";
import { fixture, root, run } from "./support";

const toolchain = readFileSync(join(root, "rust-toolchain.toml"), "utf8").match(/channel = "([^"]+)"/)![1];
const target = run(["rustc", `+${toolchain}`, "-vV"]).match(/^host: (.+)$/m)![1];
function workspace() {
  const dir = fixture("cargo graph with spaces");
  writeFileSync(join(dir, "Cargo.toml"), '[workspace]\nmembers = ["app", "shared", "leaf", "unused"]\nresolver = "2"\n');
  for (const name of ["app", "shared", "leaf", "unused"]) {
    mkdirSync(join(dir, name, "src"), { recursive: true });
    writeFileSync(join(dir, name, "Cargo.toml"), `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2024"\n`);
    writeFileSync(join(dir, name, "src/lib.rs"), "pub fn value() -> u32 { 7 }\n");
  }
  writeFileSync(join(dir, "app/Cargo.toml"), readFileSync(join(dir, "app/Cargo.toml"), "utf8") + '[dependencies]\nmodel = { package = "shared", path = "../shared", default-features = false }\n[features]\nextra = ["model/extra"]\n');
  writeFileSync(join(dir, "shared/Cargo.toml"), readFileSync(join(dir, "shared/Cargo.toml"), "utf8") + '[dependencies]\nleaf = { path = "../leaf" }\n[features]\nextra = []\n');
  run(["cargo", `+${toolchain}`, "generate-lockfile", "--offline", "--manifest-path", join(dir, "Cargo.toml")]);
  return { dir, manifestPath: join(dir, "app/Cargo.toml"), toolchain, target };
}

test("Cargo plans local libraries in dependency order with aliases and selected features", async () => {
  const options = workspace();
  const lock = readFileSync(join(options.dir, "Cargo.lock"), "utf8");
  const plan = await planCargoLibraries({ ...options, features: ["extra"] });
  expect(plan.libraries.map(p => p.name)).toEqual(["leaf", "shared", "app"]);
  expect(plan.libraries[1].features).toContain("extra");
  expect(plan.libraries[2].dependencies).toEqual([{ name: "model", packageId: plan.libraries[1].id }]);
  expect(plan.root).toBe(plan.libraries[2].id);
  expect(plan.libraries[0].sourcePath).toBe(join(options.dir, "leaf/src/lib.rs"));
  expect(readFileSync(join(options.dir, "Cargo.lock"), "utf8")).toBe(lock);
  const plain = await planCargoLibraries(options);
  expect(plain.libraries[1].features).not.toContain("extra");
});

test("Cargo virtual workspaces require selection and build scripts are rejected without executing", async () => {
  const options = workspace();
  await expect(planCargoLibraries({ ...options, manifestPath: join(options.dir, "Cargo.toml") })).rejects.toThrow("Select one");
  const selected = await planCargoLibraries({ ...options, manifestPath: join(options.dir, "Cargo.toml"), packageName: "app" });
  expect(selected.libraries.map(p => p.name)).toEqual(["leaf", "shared", "app"]);
  writeFileSync(join(options.dir, "leaf/build.rs"), 'compile_error!("must never execute");');
  await expect(planCargoLibraries(options)).rejects.toThrow("does not support build scripts: leaf");
});

test("Cargo planning rejects floating toolchains and unspecified targets", async () => {
  await expect(planCargoLibraries({ manifestPath: "Cargo.toml", toolchain: "nightly", target })).rejects.toThrow("exact toolchain pin");
  await expect(planCargoLibraries({ manifestPath: "Cargo.toml", toolchain, target: "" })).rejects.toThrow("explicit target");
});

test("Cargo ignores development dependencies but rejects reachable procedural macros", async () => {
  const options = workspace();
  const app = join(options.dir, "app/Cargo.toml");
  writeFileSync(app, readFileSync(app, "utf8") + '[dev-dependencies]\nunused = { path = "../unused" }\n');
  writeFileSync(join(options.dir, "unused/build.rs"), 'compile_error!("dev dependency must not execute");');
  run(["cargo", `+${toolchain}`, "generate-lockfile", "--offline", "--manifest-path", options.manifestPath]);
  expect((await planCargoLibraries(options)).libraries.map(p => p.name)).toEqual(["leaf", "shared", "app"]);
  const leaf = join(options.dir, "leaf/Cargo.toml");
  writeFileSync(leaf, readFileSync(leaf, "utf8") + '[lib]\nproc-macro = true\n');
  await expect(planCargoLibraries(options)).rejects.toThrow("does not support procedural macros: leaf");
});

test("Cargo refuses stale locks instead of silently changing resolution", async () => {
  const options = workspace();
  const leaf = join(options.dir, "leaf/Cargo.toml");
  writeFileSync(leaf, readFileSync(leaf, "utf8").replace('version = "0.1.0"', 'version = "0.2.0"'));
  const lock = readFileSync(join(options.dir, "Cargo.lock"), "utf8");
  await expect(planCargoLibraries(options)).rejects.toThrow();
  expect(readFileSync(join(options.dir, "Cargo.lock"), "utf8")).toBe(lock);
});

test("Cargo filters target-only dependencies for the selected target", async () => {
  const options = workspace();
  const app = join(options.dir, "app/Cargo.toml");
  writeFileSync(app, readFileSync(app, "utf8") + '\n[target.\'cfg(target_os = "none")\'.dependencies]\nunused = { path = "../unused" }\n');
  writeFileSync(join(options.dir, "unused/build.rs"), 'compile_error!("other-target dependency must not execute");');
  run(["cargo", `+${toolchain}`, "generate-lockfile", "--offline", "--manifest-path", options.manifestPath]);
  expect((await planCargoLibraries(options)).libraries.map(p => p.name)).toEqual(["leaf", "shared", "app"]);
});
