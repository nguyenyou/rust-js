// The compiler packaged is the native binary, never a launcher script: a
// launcher packaged as the compiler starts itself, again and again.

import { expect, test } from "bun:test";
import { chmodSync, existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { buildCompiler, compiler, fixture, root } from "./support";

test("a script isn't packaged as the compiler", () => {
  buildCompiler();
  const dir = fixture("package-script");
  const launcher = join(dir, "rust-js");
  // A launcher that says it's this checkout's compiler, as an installed one does.
  const identity = Bun.spawnSync([compiler, "--version-json"]).stdout.toString().trim();
  writeFileSync(launcher, `#!/bin/sh\necho '${identity}'\n`);
  chmodSync(launcher, 0o755);
  const out = join(dir, "native.tgz");
  const p = Bun.spawnSync([process.execPath, "scripts/package-compiler.ts", launcher, out], { cwd: root, stdout: "pipe", stderr: "pipe" });
  expect(p.exitCode).not.toBe(0);
  expect(p.stderr.toString()).toContain("is a script, as an installed launcher is: package the native binary");
  expect(existsSync(out)).toBe(false);
});
