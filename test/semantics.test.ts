import { beforeAll, expect, test } from "bun:test";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCompiler, compiler, fixture, root, run } from "./support";

const cases: [string, boolean | number][] = [
  ...["entry_default", "entry_trait_assignment", "entry_eager", "entry_overwrite", "entry_lazy", "checked_overwrite"].flatMap(name =>
    [false, true].map(present => [name, present] as [string, boolean])),
  ...["generic_clones", "clones", "rebuilt_clones", "clone_evaluation"].flatMap(name =>
    [0, 1, 3].map(n => [name, n] as [string, number])),
];
let generated: Record<string, (arg: any) => number[]>;
let expected: (number[] | "panic")[];
beforeAll(async () => {
  buildCompiler();
  const dir = fixture("semantics");
  copyFileSync(join(root, "test/semantics.rs"), join(dir, "cases.rs"));
  writeFileSync(join(dir, "native.rs"), `mod cases;
fn main() { ${cases.map(([name, arg]) => `
  match std::panic::catch_unwind(|| cases::${name}(${arg})) {
    Ok(value) => println!("{:?}", value),
    Err(_) => println!("\\\"panic\\\""),
  }`).join("\n")} }`);
  run(["rustc", "--edition=2024", "-Coverflow-checks=off", "-Awarnings", join(dir, "native.rs"), "-o", join(dir, "native")]);
  expected = run([join(dir, "native")]).trim().split("\n").map(line => JSON.parse(line));
  run([compiler, join(dir, "cases.rs"), "-o", join(dir, "cases.js")]);
  generated = await import(join(dir, "cases.js"));
}, 600_000);

for (const [index, [name, arg]] of cases.entries()) {
  test(`${name}(${arg}) preserves native values, effects and panics`, () => {
    let actual: number[] | "panic";
    try { actual = generated[name](arg); } catch { actual = "panic"; }
    expect(actual).toEqual(expected[index]);
  });
}

for (const collection of ["BTreeMap", "BTreeSet", "HashMap", "HashSet"]) {
  test(`${collection} rejects enum keys with custom equality`, () => {
    const dir = fixture("key-equality");
    const source = `
      #[derive(Clone, Copy, Eq, Hash)] enum Key { A, B }
      impl PartialEq for Key { fn eq(&self, _: &Self) -> bool { true } }
      impl Ord for Key { fn cmp(&self, _: &Self) -> std::cmp::Ordering { std::cmp::Ordering::Equal } }
      impl PartialOrd for Key { fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(rhs)) } }
      pub fn size() -> usize {
        let mut keys = std::collections::${collection}::new();
        keys.insert(Key::A${collection.endsWith("Map") ? ", 1" : ""});
        keys.insert(Key::B${collection.endsWith("Map") ? ", 2" : ""});
        keys.len()
      }`;
    const input = join(dir, "keys.rs"), output = join(dir, "keys.js");
    writeFileSync(input, source);
    writeFileSync(output, "last successful build");
    const result = Bun.spawnSync([compiler, input, "-o", output]);
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain("does not support values of type `Key`");
    expect(readFileSync(output, "utf8")).toBe("last successful build");
  });
}

test("BTreeMap rejects custom ordering even when equality is derived", () => {
  const dir = fixture("key-ordering");
  const input = join(dir, "keys.rs"), output = join(dir, "keys.js");
  writeFileSync(input, `
    #[derive(Clone, Copy, Eq, PartialEq)] enum Key { A, B }
    impl Ord for Key { fn cmp(&self, other: &Self) -> std::cmp::Ordering { (*other as u8).cmp(&(*self as u8)) } }
    impl PartialOrd for Key { fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) } }
    pub fn size() -> usize { let m = std::collections::BTreeMap::from([(Key::A, 1), (Key::B, 2)]); m.len() }
  `);
  const result = Bun.spawnSync([compiler, input, "-o", output]);
  expect(result.exitCode).not.toBe(0);
  expect(result.stderr.toString()).toContain("does not support values of type `Key`");
});
