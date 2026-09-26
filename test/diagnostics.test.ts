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
  ["byte offsets of a string", 'pub fn f(s: &str) -> Option<usize> { s.find(\'o\') }', "`find()` of a string"],
  ["binary_search of floats", 'pub fn f(v: &[f64]) -> bool { v.binary_search_by(|x| x.total_cmp(&1.0)).is_ok() }', "does not support"],
  ["an operator in generic code", 'pub fn f<T: std::ops::Add<Output = T>>(a: T, b: T) -> T { a + b }', "does not support"],
  ["then to a nullish type", 'pub fn f(b: bool) -> bool { b.then(|| ()).is_some() }', "values of type `std::option::Option<()>`"],
  ["a reference count", 'pub fn f(r: &std::rc::Rc<u32>) -> usize { std::rc::Rc::strong_count(r) }', "does not support"],
  ["a heap of options", 'pub fn f() -> bool { let mut h = std::collections::BinaryHeap::new(); h.push(Some(1u32)); h.pop().is_some() }', "a heap of"],
  ["IndexMut of the crate's own", 'pub struct G(Vec<u32>);\nimpl std::ops::Index<usize> for G { type Output = u32; fn index(&self, i: usize) -> &u32 { &self.0[i] } }\nimpl std::ops::IndexMut<usize> for G { fn index_mut(&mut self, i: usize) -> &mut u32 { &mut self.0[i] } }', "user implementations of this standard or external trait"],
  ["comparing another crate's struct", 'pub fn f(a: std::time::Duration, b: std::time::Duration) -> bool { a < b }', "does not support"],
  ["next() of an iterator in a field", 'pub struct L<\'a> { c: std::str::Chars<\'a> }\npub fn f(l: &mut L) -> Option<char> { l.c.next() }', "make it a `Peekable`"],
  ["peekable of a lazy iterator", 'pub struct C(u32);\nimpl Iterator for C { type Item = u32; fn next(&mut self) -> Option<u32> { self.0 += 1; Some(self.0) } }\npub fn f() -> Option<u32> { let mut p = C(0).peekable(); p.peek().copied() }', "`peekable` of a lazy iterator"],
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
