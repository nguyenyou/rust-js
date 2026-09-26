import { beforeAll, expect, test } from "bun:test";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCompiler, compiler, fixture, run } from "./support";

beforeAll(buildCompiler, 600_000);

test("late import aliases avoid locals, nested parameters and generated bindings", async () => {
  const dir = fixture("link-names");
  const source = `
    pub mod util { pub fn add(n: i32) -> i32 { n + 1 } }
    pub mod value { pub fn add(n: i32) -> i32 { super::util::add(n) + 1 } }
    pub fn run() -> i32 {
      let util = 10;
      let value = 20;
      let f = |util: i32| self::util::add(util) + self::value::add(value);
      f(util)
    }
    pub fn text() -> String { format!("answer={}", util::add(1)) }
  `;
  writeFileSync(join(dir, "lib.rs"), source);
  writeFileSync(join(dir, "native.rs"), 'mod lib; fn main() { println!("{}", lib::run()); println!("{}", lib::text()); }');
  run(["rustc", "--edition=2024", "-Awarnings", join(dir, "native.rs"), "-o", join(dir, "native")]);
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
  const js = await import(join(dir, "lib.js"));
  expect([String(js.run()), js.text()]).toEqual(run([join(dir, "native")]).trim().split("\n"));
  const code = readFileSync(join(dir, "lib.js"), "utf8");
  expect(code).toContain("import * as util$2");
  expect(code).toContain("import * as value$1");
  expect(code).not.toContain("\0");
});

test("a sparse cyclic graph emits only each module's actual imports", async () => {
  const dir = fixture("link-graph"), count = 32;
  writeFileSync(join(dir, "lib.rs"), Array.from({ length: count }, (_, i) => `
    pub mod m${i} { pub fn f(n: u32) -> u32 { if n == 0 { ${i} } else { super::m${(i + 1) % count}::f(n - 1) } } }
  `).join("\n"));
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
  const first = await import(join(dir, "m0.js"));
  expect(first.f(47)).toBe(15);
  for (const file of readdirSync(dir).filter(name => /^m\d+\.js$/.test(name))) {
    const code = readFileSync(join(dir, file), "utf8");
    expect(code.match(/^import /gm)?.length).toBe(1);
    expect(code).not.toContain("\0");
  }
});
