import { beforeAll, expect, test } from "bun:test";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCompiler, compiler, fixture } from "./support";

beforeAll(buildCompiler, 600_000);

for (const [name, source, message] of [
  ["type error", 'pub fn f() -> i32 { "wrong" }', "mismatched types"],
  ["borrow error", 'pub fn f() -> i32 { let mut x = 1; let r = &x; x = 2; *r }', "borrowed"],
  ["unsupported type", 'pub fn f(x: u64) -> u64 { x }', "does not support"],
  ["camelCase fields that collide", '#![rust_js::camel_case]\n#![allow(non_snake_case)]\npub struct P { pub first_name: u32, pub firstName: u32 }\npub fn f(p: &P) -> u32 { p.first_name + p.firstName }', "both `firstName` in JS"],
  ["associated constant", "pub trait Area { const UNIT: u32; }", "does not support associated constants"],
  ["option of a reference to unit", "pub fn f(x: &()) -> bool { Some(x).is_some() }", "does not support values of type"],
  ["map to a nullish type", 'pub fn f(o: Option<i32>) -> bool { o.map(|_| ()).is_some() }', "`map` to a `()`"],
  ["precision of a struct", 'pub struct P; impl std::fmt::Display for P { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("p") } }\npub fn f() -> String { format!("{:.2}", P) }', "a precision for a"],
  ["map with struct keys", '#[derive(PartialEq, Eq, Hash)] pub struct P { pub x: u32 }\npub fn f() -> usize { let m: std::collections::HashMap<P, u32> = std::collections::HashMap::new(); m.len() }', "does not support values of type `P`"],
  ["== on maps", 'pub fn f(a: &std::collections::HashMap<u32, u32>, b: &std::collections::HashMap<u32, u32>) -> bool { a == b }', "`==` on"],
  ["parse to a type without FromStr support", 'pub fn f(s: &str) -> bool { s.parse::<std::net::IpAddr>().is_ok() }', "does not support"],
  ["slicing by a range in a variable", 'pub fn f(v: &[u32], r: std::ops::Range<usize>) -> usize { v[r].len() }', "slicing by a range in a variable"],
  ["malformed import", '#![rust_js::import("./style.css")]\npub fn f() {}', "write it"],
  ["malformed binding", '#[rust_js::link_name(123)] pub fn f() {}', "a binding needs"],
  ["invalid JSX binding", '#[rust_js::link_name = "<div>"] fn div(a: i32, b: i32) -> i32 { unreachable!() }\npub fn f() -> i32 { div(1, 2) }', "JSX binding"],
]) {
  test(`${name} reports a source location and preserves existing output`, () => {
    const dir = fixture("diagnostic");
    const input = join(dir, "lib.rs"), output = join(dir, "lib.js"), manifest = join(dir, "manifest.json");
    writeFileSync(input, source);
    const args = [compiler, input, "-o", output, "--manifest", manifest];
    const rejected = Bun.spawnSync(args);
    expect(rejected.exitCode).not.toBe(0);
    expect(rejected.stderr.toString()).toContain(message);
    expect(rejected.stderr.toString()).toContain("lib.rs:");
    expect(existsSync(output)).toBe(false);
    expect(existsSync(manifest)).toBe(false);
    writeFileSync(output, "last successful build");
    writeFileSync(output + ".map", "previous map");
    expect(Bun.spawnSync(args).exitCode).not.toBe(0);
    expect(readFileSync(output, "utf8")).toBe("last successful build");
    expect(readFileSync(output + ".map", "utf8")).toBe("previous map");
  });
}
