import { beforeAll, expect, test } from "bun:test";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCompiler, buildSerde, compiler, fixture } from "./support";

beforeAll(buildCompiler, 600_000);

for (const [name, source, message, crate] of [
  ["type error", 'pub fn f() -> i32 { "wrong" }', "mismatched types"],
  ["borrow error", 'pub fn f() -> i32 { let mut x = 1; let r = &x; x = 2; *r }', "borrowed"],
  ["unsupported type", 'pub fn f(x: u128) -> u128 { x }', "does not support"],
  ["camelCase fields that collide", '#![rust_js::camel_case]\n#![allow(non_snake_case)]\npub struct P { pub first_name: u32, pub firstName: u32 }\npub fn f(p: &P) -> u32 { p.first_name + p.firstName }', "both `firstName` in JS"],
  ["associated constant", "pub trait Area { const UNIT: u32; }", "does not support associated constants"],
  ["#[thread_local] static", "#![feature(thread_local)]\n#[thread_local] static N: std::cell::Cell<u32> = std::cell::Cell::new(0);\npub fn f() -> u32 { N.get() }", "does not support `#[thread_local]` statics"],
  ["static holding a reference to another", "static A: u32 = 1;\nstatic B: &u32 = &A;\npub fn f() -> u32 { *B }", "does not support statics of type `&'static u32`"],
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
  ["IndexMut to a number", 'pub struct G(Vec<u32>);\nimpl std::ops::Index<usize> for G { type Output = u32; fn index(&self, i: usize) -> &u32 { &self.0[i] } }\nimpl std::ops::IndexMut<usize> for G { fn index_mut(&mut self, i: usize) -> &mut u32 { &mut self.0[i] } }', "does not support values of type `&mut u32`"],
  ["a pointer of the crate's own to a dyn", "#![feature(derive_coerce_pointee)]\nuse std::ops::Deref;\n#[derive(std::marker::CoercePointee)] #[repr(transparent)] pub struct Ptr<'a, #[pointee] T: ?Sized> { ptr: &'a T }\nimpl<T: ?Sized> Deref for Ptr<'_, T> { type Target = T; fn deref(&self) -> &T { self.ptr } }\npub trait Get { fn get(&self) -> u32; }\npub struct V(u32);\nimpl Get for V { fn get(&self) -> u32 { self.0 } }\npub fn f() -> u32 { let v = V(10); let p: Ptr<dyn Get> = Ptr { ptr: &v }; p.get() }", "does not support unsizing a `Ptr<'_, dyn Get>`"],
  ["a temporary with a destructor", 'pub struct D;\nimpl Drop for D { fn drop(&mut self) {} }\nimpl D { pub fn n(&self) -> u32 { 1 } }\npub fn f() -> u32 { D.n() }', "does not support a temporary with a destructor"],
  ["an Rc of a value with a destructor", 'pub struct D;\nimpl Drop for D { fn drop(&mut self) {} }\npub fn f() { let r = std::rc::Rc::new(D); drop(r); }', "a std type holding a value with a destructor, `std::rc::Rc<D>`"],
  ["a user impl of a std trait", 'pub struct C;\nimpl std::hash::Hasher for C { fn finish(&self) -> u64 { 0 } fn write(&mut self, _: &[u8]) {} }', "user implementations of this standard or external trait"],
  ["comparing another crate's struct", 'pub fn f(a: std::time::Duration, b: std::time::Duration) -> bool { a < b }', "does not support"],
  ["next() of an iterator in a field", 'pub struct L<\'a> { c: std::str::Chars<\'a> }\npub fn f(l: &mut L) -> Option<char> { l.c.next() }', "make it a `Peekable`"],
  ["peekable of a lazy iterator", 'pub struct C(u32);\nimpl Iterator for C { type Item = u32; fn next(&mut self) -> Option<u32> { self.0 += 1; Some(self.0) } }\npub fn f() -> Option<u32> { let mut p = C(0).peekable(); p.peek().copied() }', "`peekable` of a lazy iterator"],
  ["a &mut to a number returned", 'pub fn pick(a: &mut u32) -> &mut u32 { a }', "does not support"],
  ["a &mut in a variable, kept in a struct", 'pub struct H<\'a> { pub r: &\'a mut u32 }\npub fn f() -> u32 { let mut a = 1; let x = &mut a; let h = H { r: x }; *h.r }', "does not support values of type `&mut u32`"],
  ["a &mut in a variable passed as a generic value", 'pub trait Bump { fn bump(self); }\nimpl Bump for &mut i32 { fn bump(self) { *self += 1; } }\nfn go<T: Bump>(t: T) { t.bump() }\npub fn f() -> i32 { let mut x = 1; let y = &mut x; go(y); x }', "a `&mut` in a variable used as a value"],
  ["a &mut to a closure of a type parameter", 'pub fn call<F: FnMut()>(f: &mut F) { f() }', "does not support values of type `&mut F`"],
  ["a &mut to an iterator of a type parameter", 'pub fn first<I: Iterator<Item = i32>>(it: &mut I) -> Option<i32> { it.next() }', "does not support values of type `&mut I`"],
  ["a &mut chosen by a branch", 'pub fn f(c: bool) -> u32 { let (mut a, mut b) = (1, 2); let x = if c { &mut a } else { &mut b }; *x = 0; a + b }', "does not support values of type `&mut u32`"],
  ["ref mut through a reference variable, replaced whole", 'pub struct P { pub x: u32 }\n#[allow(unused_mut)] pub fn f() -> u32 { let mut a = P { x: 1 }; let mut cur = &mut a; match *cur { ref mut n => *n = P { x: 2 } } a.x }', "assigning a whole value through a `&mut`"],
  ["{:.2e}", 'pub fn f(x: f64) -> String { format!("{:.2e}", x) }', "`{:.2e}` and the like"],
  ["malformed import", '#![rust_js::import("./style.css")]\npub fn f() {}', "write it"],
  ["malformed binding", '#[rust_js::link_name(123)] pub fn f() {}', "a binding needs"],
  ["handwritten JSX binding", '#[rust_js::link_name = "<div>"] fn div(a: i32, b: i32) -> i32 { unreachable!() }\npub fn f() -> i32 { div(1, 2) }', "element builders are compiler-only"],
  ["#[serde(with)]", 'mod m { pub fn serialize<S: serde::Serializer>(v: &u32, s: S) -> Result<S::Ok, S::Error> { s.serialize_u32(*v) } }\n#[derive(serde::Serialize)] pub struct W { #[serde(with = "m")] pub x: u32 }\npub fn f(w: &W) -> String { serde_json::to_string(w).unwrap() }', "`#[serde(with)]`", "serde"],
  ["#[serde(serialize_with)]", 'fn s<S: serde::Serializer>(v: &u32, s: S) -> Result<S::Ok, S::Error> { s.serialize_u32(*v) }\n#[derive(serde::Serialize)] pub struct W { #[serde(serialize_with = "s")] pub x: u32 }\npub fn f(w: &W) -> String { serde_json::to_string(w).unwrap() }', "`#[serde(serialize_with)]`", "serde"],
  ["#[serde(deserialize_with)]", 'fn d<\'de, D: serde::Deserializer<\'de>>(d: D) -> Result<u32, D::Error> { <u32 as serde::Deserialize>::deserialize(d) }\n#[derive(serde::Deserialize)] pub struct W { #[serde(deserialize_with = "d")] pub x: u32 }\npub fn f(s: &str) -> bool { serde_json::from_str::<W>(s).is_ok() }', "`#[serde(deserialize_with)]`", "serde"],
  ["reading a u128", 'pub fn f(s: &str) -> bool { serde_json::from_str::<u128>(s).is_ok() }', "does not support", "serde"],
  ["reading a BinaryHeap", 'pub fn f(s: &str) -> bool { serde_json::from_str::<std::collections::BinaryHeap<u32>>(s).is_ok() }', "deserializing", "serde"],
  ["unsupported Value method", 'pub fn f(v: &serde_json::Value) -> bool { v.pointer("/name").is_some() }', "`Value::pointer`", "serde"],
  ["assigning to a Value's key", 'pub fn f() -> String { let mut v = serde_json::json!({}); v["k"] = serde_json::json!(1); v.to_string() }', "assigning to this place", "serde"],
] as [string, string, string, string?][]) {
  test(`${name} reports a source location and preserves existing output`, () => {
    const dir = fixture("diagnostic");
    const input = join(dir, "lib.rs"), output = join(dir, "lib.js"), manifest = join(dir, "manifest.json");
    writeFileSync(input, source);
    const args = [compiler, input, "-o", output, "--manifest", manifest, ...(crate ? ["--", ...buildSerde()] : [])];
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
