// What a release publishes to npm: the distribution's packages, the
// compiler and what an app installs with it (ADR 0094), and @rust-js/create
// (ADR 0105), each publishable, not `private`, which npm refuses, at the
// compiler's version, and each of rust-js's packages it depends on at that
// version.

import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, realpathSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildCompiler, compiler, fixture, root, run } from "./support";

const version = (Bun.TOML.parse(readFileSync(join(root, "Cargo.toml"), "utf8")) as { package: { version: string } }).package.version;

test("each package a release publishes is publishable, at the compiler's version", () => {
  buildCompiler();
  const dir = realpathSync(mkdtempSync(join(tmpdir(), "rust-js-release-")));
  const bundle = join(dir, "distribution");
  run([process.execPath, "scripts/package-distribution.ts", compiler, bundle], 600_000);
  run([process.execPath, "scripts/package-create.ts", join(bundle, "create.tgz")]);
  const names: string[] = [];
  for (const file of ["runtime.tgz", "resources.tgz", "native.tgz", "build.tgz", "vite-plugin.tgz", "create.tgz"]) {
    const unpacked = fixture(`release-${file}`);
    run(["tar", "-xzf", join(bundle, file), "-C", unpacked]);
    const pkg = JSON.parse(readFileSync(join(unpacked, "package", "package.json"), "utf8"));
    names.push(pkg.name);
    expect([pkg.name, pkg.private]).toEqual([pkg.name, undefined]);
    expect([pkg.name, pkg.version]).toEqual([pkg.name, version]);
    expect([pkg.name, pkg.license]).toEqual([pkg.name, "MIT"]);
    expect([pkg.name, pkg.repository?.url]).toEqual([pkg.name, "git+https://github.com/rust-js-lang/rust-js.git"]);
    for (const field of ["dependencies", "peerDependencies", "optionalDependencies"]) {
      for (const [dependency, range] of Object.entries(pkg[field] ?? {})) {
        if (dependency.startsWith("@rust-js/")) expect([pkg.name, dependency, range]).toEqual([pkg.name, dependency, version]);
      }
    }
  }
  expect(names.sort()).toEqual(["@rust-js/build", "@rust-js/create", "@rust-js/native", "@rust-js/resources", "@rust-js/runtime", "@rust-js/vite-plugin"]);
}, 900_000);
