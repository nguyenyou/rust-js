// The runtime as a package (ADR 0103): `@rust-js/runtime`, released with
// the compiler, as ReScript's `@rescript/runtime` is. A module imports the
// helpers its code names from it, instead of carrying its own copy of each.

import { beforeAll, expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { node } from "./programs";
import { buildCompiler, compiler, fixture, root, run } from "./support";

beforeAll(buildCompiler, 600_000);

// The package's module is the compiler's helpers, each exported: made by
// the compiler, so it can't drift from what the compiler's imports name.
test("the committed @rust-js/runtime is the compiler's helpers, each exported once", () => {
  const printed = run([compiler, "--runtime-module"]);
  expect(readFileSync(join(root, "runtime", "index.js"), "utf8")).toBe(printed);
  const names = [...printed.matchAll(/^export (?:async )?(?:function\*?|class|const|let) (\$[\w$]+)/gm)].map((m) => m[1]);
  expect(names.length).toBeGreaterThan(100);
  expect(new Set(names).size).toBe(names.length);
  const pkg = JSON.parse(readFileSync(join(root, "runtime", "package.json"), "utf8"));
  const version = readFileSync(join(root, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)![1];
  expect([pkg.name, pkg.version]).toEqual(["@rust-js/runtime", version]);
});

// A module names a few helpers, and imports just those; one a helper uses,
// the package has for it.
test("a module compiled with --runtime-package imports the helpers it names, and runs", () => {
  const dir = fixture("runtime-package");
  writeFileSync(join(dir, "lib.rs"), `pub fn main() {
    let names = vec!["ada", "grace"];
    let i = names.len() - 1;
    println!("{:?} {}", names[i], 7 / (i as i32));
    let parsed: Result<u32, _> = "x".parse();
    println!("{:?}", parsed.is_err());
}
`);
  const out = join(dir, "lib.js");
  run([compiler, join(dir, "lib.rs"), "-o", out, "--runtime-package"]);
  const js = readFileSync(out, "utf8");
  expect(js).toMatch(/^import \{ [^}]+ \} from "@rust-js\/runtime";$/m);
  expect(js).not.toMatch(/^function \$/m);
  const imported = js.match(/^import \{ ([^}]+) \} from "@rust-js\/runtime";$/m)![1].split(", ");
  expect(imported).toEqual([...imported].sort());
  for (const name of imported) expect(js.split("\n").slice(3).join("\n")).toContain(name);
  const printed = run([node ?? "node", "--input-type=module", "--eval", `(await import(${JSON.stringify(out)})).main();`]);
  expect(printed).toBe('"grace" 7\ntrue\n');
});
