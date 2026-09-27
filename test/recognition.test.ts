import { beforeAll, expect, test } from "bun:test";
import { join } from "node:path";
import { readFileSync, writeFileSync } from "node:fs";
import { buildCompiler, compiler, fixture, run } from "./support";

beforeAll(buildCompiler, 600_000);

test("library recognition distinguishes user lookalikes and preserves standard behavior", async () => {
  const dir = fixture("recognition");
  const source = `
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reverse(pub i32);
#[derive(Debug)] pub struct ParseIntError;
#[derive(Debug)] pub struct Error;
#[derive(Debug)] pub struct Split { pub value: u32 }
pub fn user_types() -> String {
    format!("{:?}|{:?}|{:?}|{:?}|{}", Reverse(3), ParseIntError, Error, Split { value: 7 }, Reverse(1) < Reverse(2))
}
pub fn standard_types() -> String {
    format!("{:?}|{}|{}", "".parse::<i32>().unwrap_err(), std::cmp::Reverse(1) < std::cmp::Reverse(2), "a b c".split_whitespace().count())
}
pub fn comparisons() -> String {
    let a = Reverse(1); let b = Reverse(2);
    format!("{}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}", a < b, a <= b, a > b, a >= b, a.cmp(&b), a.partial_cmp(&b), a.clone().max(b.clone()), a.min(b))
}
`;
  const input = join(dir, "cases.rs"), output = join(dir, "cases.js");
  writeFileSync(input, source);
  writeFileSync(join(dir, "main.rs"), 'mod cases; fn main() { println!("{}\\n{}\\n{}", cases::user_types(), cases::standard_types(), cases::comparisons()); }');
  run(["rustc", "--edition=2024", join(dir, "main.rs"), "-o", join(dir, "native")]);
  const expected = run([join(dir, "native")]).trim().split("\n");
  run([compiler, input, "-o", output]);
  const compiled = await import(output);
  expect([compiled.user_types(), compiled.standard_types(), compiled.comparisons()]).toEqual(expected);
  expect(readFileSync(output, "utf8")).toContain("$debugParseError");
}, 120_000);
