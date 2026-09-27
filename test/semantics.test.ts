import { beforeAll, expect, test } from "bun:test";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { decode, expected, observe, type Outcome } from "./oracle";
import { buildCompiler, compiler, fixture, root, run } from "./support";

const cases: [string, boolean | number][] = [
  ...["entry_default", "entry_trait_assignment", "entry_eager", "entry_overwrite", "entry_lazy", "checked_overwrite"].flatMap(name =>
    [false, true].map(present => [name, present] as [string, boolean])),
  ...["generic_clones", "clones", "rebuilt_clones", "clone_evaluation"].flatMap(name =>
    [0, 1, 3].map(n => [name, n] as [string, number])),
  ...["nested_struct_literal", "struct_in_base_block", "discarded_nested_box", "operand_prerequisites"].flatMap(name =>
    [0, 10].map(n => [name, n] as [string, number])),
  ...["nested_struct_updates", "discarded_nested_insert"].flatMap(name =>
    [false, true].map(value => [name, value] as [string, boolean])),
];
let generated: Record<string, (arg: any) => number[]>;
let native: Outcome[];
beforeAll(async () => {
  buildCompiler();
  const dir = fixture("semantics");
  copyFileSync(join(root, "test/semantics.rs"), join(dir, "cases.rs"));
  // Each outcome as a JSON line: the value, or the panic's message, which
  // the JS must match exactly. The messages here are plain text, which
  // `{:?}` writes as JSON does.
  writeFileSync(join(dir, "native.rs"), `mod cases;
fn message(e: Box<dyn std::any::Any + Send>) -> String {
  e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned())
    .expect("a panic with a message")
}
fn main() { ${cases.map(([name, arg]) => `
  match std::panic::catch_unwind(|| cases::${name}(${arg})) {
    Ok(value) => println!("{{\\"value\\":{:?}}}", value),
    Err(e) => println!("{{\\"panic\\":{:?}}}", message(e)),
  }`).join("\n")} }`);
  run(["rustc", "--edition=2024", "-Coverflow-checks=off", "-Awarnings", join(dir, "native.rs"), "-o", join(dir, "native")]);
  native = run([join(dir, "native")]).trim().split("\n").map(line => expected(decode(line)));
  run([compiler, join(dir, "cases.rs"), "-o", join(dir, "cases.js")]);
  generated = await import(join(dir, "cases.js"));
}, 600_000);

for (const [index, [name, arg]] of cases.entries()) {
  test(`${name}(${arg}) preserves native values, effects and panics`, () => {
    expect(observe(() => generated[name](arg))).toStrictEqual(native[index]);
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
