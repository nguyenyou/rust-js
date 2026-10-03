import { beforeAll, expect, test } from "bun:test";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCompiler, buildSerde, compiler, fixture, root } from "./support";

beforeAll(buildCompiler, 600_000);

for (const [name, source, message, crate] of [
  ["type error", 'pub fn f() -> i32 { "wrong" }', "mismatched types"],
  ["borrow error", 'pub fn f() -> i32 { let mut x = 1; let r = &x; x = 2; *r }', "borrowed"],
  ["unsupported type", 'pub fn f(x: u128) -> u128 { x }', "does not support"],
  // ADR 0122: an `f32`'s bits aren't yet: a NaN's payload is JS's to keep or not.
  ["an f32's bits", "pub fn f(x: f32) -> u32 { x.to_bits() }", "to_bits"],
  // ADR 0123: what `..` binds is a copy, as `&v[a..b]` is (ADR 0063), so one to write through can't be.
  ["a &mut rest of a slice pattern", "pub fn f(v: &mut [u32]) { if let [_, rest @ ..] = v { rest[0] = 1; } }", "a `&mut` to part of a slice"],
  // ADR 0124: a choice of places, `p[0] === 0 ? p[1] : p[0]`, can't be written through.
  ["a ref mut bound at two places of a | pattern", "pub fn f(p: &mut (i32, i32)) { if let (0, x) | (x, 0) = p { *x += 1; } }", "a `ref mut` bound at another place in each alternative of a `|` pattern"],
  // A guard is tried with each alternative in turn, which a choice of places, the first's, isn't:
  // `(3, 42)` takes this arm in Rust, with `a` 42 (rustc's issue-70413).
  ["a guard of a | pattern binding at two places", "pub fn f(p: (i32, i32)) -> i32 { match p { (a, _) | (_, a) if a > 10 => a, _ => 0 } }", "a guard of a `|` pattern binding a name at another place in each alternative"],
  ["camelCase fields that collide", '#![allow(non_snake_case)]\n#[rust_js::camel_case]\nconst _: () = ();\npub struct P { pub first_name: u32, pub firstName: u32 }\npub fn f(p: &P) -> u32 { p.first_name + p.firstName }', "both `firstName` in JS"],
  ["#[thread_local] static", "#![feature(thread_local)]\n#[thread_local] static N: std::cell::Cell<u32> = std::cell::Cell::new(0);\npub fn f() -> u32 { N.get() }", "does not support `#[thread_local]` statics"],
  ["static holding a reference to another", "static A: u32 = 1;\nstatic B: &u32 = &A;\npub fn f() -> u32 { *B }", "does not support statics of type `&'static u32`"],
  // A `dyn Error`'s dictionary has its `source`, and none of what else std provides (ADR 0141).
  ["a deprecated method of a dyn Error", '#![allow(deprecated)]\npub fn f(e: &dyn std::error::Error) -> String { e.description().to_string() }', "`description` of a"],
  // A placeholder's options are given to what shows the value (ADR 0058), but a
  // `&dyn Debug` is the string it shows already, and serde_json's `fmt`s are rust-js's.
  ["a width for a dyn Debug", 'pub fn f(v: u8) -> String { let d: &dyn std::fmt::Debug = &v; format!("{:5?}", d) }', "options for a"],
  ["a width for a serde_json Value", 'pub fn f(v: &serde_json::Value) -> String { format!("{:>9}", v) }', "options for a", "serde"],
  // A static's initializer is code where rustc's value can't say it (ADR 0096),
  // but not one reading another static, nor of a value that can change in place.
  ["a static of a function read from another static", 'pub fn f() {}\nstatic A: fn() = f;\npub static B: &fn() = &A;\npub fn g() { B() }', "statics of type"],
  ["a constant of a Vec of functions", 'pub fn f() {}\npub const C: Vec<fn()> = Vec::new();\npub fn g() -> usize { C.len() }', "constants of type"],
  // ADR 0121: a key found by its value is one a derived `Eq` compares.
  ["map keyed by a struct of its own equality", '#[derive(Hash)] pub struct P { pub x: u32 }\nimpl PartialEq for P { fn eq(&self, o: &P) -> bool { self.x % 10 == o.x % 10 } }\nimpl Eq for P {}\npub fn f() -> usize { let m: std::collections::HashMap<P, u32> = std::collections::HashMap::new(); m.len() }', "does not support values of type `P`"],
  ["map keyed by a struct with a field of its own equality", '#[derive(Hash)] pub struct Q(pub u32);\nimpl PartialEq for Q { fn eq(&self, o: &Q) -> bool { self.0 % 10 == o.0 % 10 } }\nimpl Eq for Q {}\n#[derive(PartialEq, Eq, Hash)] pub struct P { pub q: Q }\npub fn f() -> usize { let m: std::collections::HashSet<P> = std::collections::HashSet::new(); m.len() }', "does not support values of type `P`"],
  ["B-tree keyed by a struct", '#[derive(PartialEq, Eq, PartialOrd, Ord)] pub struct P { pub x: u32 }\npub fn f() -> usize { let m: std::collections::BTreeMap<P, u32> = std::collections::BTreeMap::new(); m.len() }', "does not support values of type `P`"],
  ["== on maps", 'pub fn f(a: &std::collections::HashMap<u32, u32>, b: &std::collections::HashMap<u32, u32>) -> bool { a == b }', "`==` on"],
  ["parse to a type without FromStr support", 'pub fn f(s: &str) -> bool { s.parse::<std::net::IpAddr>().is_ok() }', "does not support"],
  ["next() of a RangeInclusive kept as a value", 'pub fn f(r: &mut std::ops::RangeInclusive<u32>) -> Option<u32> { r.next() }', "kept as a value"],
  // A string's byte offsets are counted (ADR 0138), but not yet of a closure's matches.
  ["find by a closure", 'pub fn f(s: &str) -> Option<usize> { s.find(|c: char| c == \'o\') }', "::find`"],
  ["binary_search of floats", 'pub fn f(v: &[f64]) -> bool { v.binary_search_by(|x| x.total_cmp(&1.0)).is_ok() }', "does not support"],
  // `a + b` of a `T: Add` is its dictionary's (ADR 0108); `a += b` isn't yet.
  ["an assigning operator in generic code", 'pub fn f<T: std::ops::AddAssign>(a: &mut T, b: T) { *a += b; }', "does not support"],
  ["a reference count", 'pub fn f(r: &std::rc::Rc<u32>) -> usize { std::rc::Rc::strong_count(r) }', "does not support"],
  ["a heap of options", 'pub fn f() -> bool { let mut h = std::collections::BinaryHeap::new(); h.push(Some(1u32)); h.pop().is_some() }', "a heap of"],
  ["a pointer of the crate's own to a dyn", "#![feature(derive_coerce_pointee)]\nuse std::ops::Deref;\n#[derive(std::marker::CoercePointee)] #[repr(transparent)] pub struct Ptr<'a, #[pointee] T: ?Sized> { ptr: &'a T }\nimpl<T: ?Sized> Deref for Ptr<'_, T> { type Target = T; fn deref(&self) -> &T { self.ptr } }\npub trait Get { fn get(&self) -> u32; }\npub struct V(u32);\nimpl Get for V { fn get(&self) -> u32 { self.0 } }\npub fn f() -> u32 { let v = V(10); let p: Ptr<dyn Get> = Ptr { ptr: &v }; p.get() }", "does not support unsizing a `Ptr<'_, dyn Get>`"],
  // A let-chain's condition's temporaries end at its `&&`, its `let`s' with the `if`: not yet (ADR 0098).
  ["a temporary with a destructor", 'pub struct D;\nimpl Drop for D { fn drop(&mut self) {} }\nimpl D { pub fn n(&self) -> u32 { 1 } }\npub fn f(b: bool) -> u32 { if D.n() == 1 && let true = b { 1 } else { 0 } }', "does not support a temporary with a destructor"],
  // A `dyn` of std's traits has no dictionary to carry a drop (ADR 0098).
  ["a Box<dyn Send> of a value with a destructor", 'pub struct D;\nimpl Drop for D { fn drop(&mut self) {} }\npub fn f() { let _: Box<dyn Send> = Box::new(D); }', "a `dyn` of a value with a destructor"],
  ["an Rc<dyn> of a value with a destructor", 'pub struct D;\nimpl Drop for D { fn drop(&mut self) {} }\npub fn f() { let _: std::rc::Rc<dyn Send> = std::rc::Rc::new(D); }', "a `dyn` of a value with a destructor"],
  // A std call given an iterator whose destructors rust-js can't follow would drop what it skips silently.
  ["last of owned items", 'pub struct D;\nimpl Drop for D { fn drop(&mut self) {} }\npub fn f(v: Vec<D>) -> Option<D> { v.into_iter().last() }', "`std::iter::Iterator::last` of a value with a destructor"],
  // A size of a type parameter is its caller's (ADR 0145), but `size_of_val` of an unsized one is the value's.
  ["size_of_val of an unsized type parameter", 'pub fn f<T: ?Sized>(x: &T) -> usize { std::mem::size_of_val(x) }\npub fn g() -> usize { f("ab") }', "`size_of` of a type parameter"],
  // `ToString` is a dictionary of a generic `T`'s, but a `dyn ToString` has no pair to call through.
  ["to_string of a dyn ToString", 'pub fn f(x: &dyn ToString) -> String { x.to_string() }', "dyn std::string::ToString"],
  // `collect()` into a `Result` or an `Option` is of an array (ADR 0036).
  ["collecting into a Result of a String", 'pub fn f(v: Vec<Result<char, ()>>) -> Result<String, ()> { v.into_iter().collect() }', "collecting into a"],
  // `len()` takes no item, so it runs none of a chain's closures (ADR 0036).
  ["len of a chain whose closure prints", 'pub fn f(v: &[u32]) -> usize { v.iter().map(|x| { println!("{x}"); x }).len() }', "`len()` of an iterator whose closures do what can be seen"],
  ["len of a Peekable", 'pub fn f(v: &[u32]) -> usize { let mut p = v.iter().peekable(); p.peek(); p.len() }', "`len()` of a `Peekable`"],
  ["an Rc of a value with a destructor", 'pub struct D;\nimpl Drop for D { fn drop(&mut self) {} }\npub fn f() { let r = std::rc::Rc::new(D); drop(r); }', "a std type holding a value with a destructor, `std::rc::Rc<D>`"],
  ["a user impl of a std trait", 'pub struct C;\nimpl std::hash::Hasher for C { fn finish(&self) -> u64 { 0 } fn write(&mut self, _: &[u8]) {} }', "user implementations of this standard or external trait"],
  ["comparing another crate's struct", 'pub fn f(a: std::time::Duration, b: std::time::Duration) -> bool { a < b }', "does not support"],
  ["next() of an iterator in a field", 'pub struct L<\'a> { c: std::str::Chars<\'a> }\npub fn f(l: &mut L) -> Option<char> { l.c.next() }', "make it a `Peekable`"],
  ["peekable of a lazy iterator", 'pub struct C(u32);\nimpl Iterator for C { type Item = u32; fn next(&mut self) -> Option<u32> { self.0 += 1; Some(self.0) } }\npub fn f() -> Option<u32> { let mut p = C(0).peekable(); p.peek().copied() }', "`peekable` of a lazy iterator"],
  ["a new closure assigned through a &mut to one", 'pub fn replace<F: FnMut()>(f: &mut F, g: F) { *f = g; }', "assigning a whole value through a `&mut`"],
  ["a std &mut to a number in a variable", "use std::collections::HashMap;\npub fn f(m: &mut HashMap<u32, i32>) -> i32 { let r = m.get_mut(&1); match r { Some(x) => *x, None => 0 } }", "a `&mut i32` from `std::collections::HashMap::<K, V, S, A>::get_mut` used as a value"],
  ["a std &mut to a number passed on", "use std::collections::HashMap;\nfn g(o: Option<&mut i32>) -> i32 { o.map_or(0, |x| *x) }\npub fn f(m: &mut HashMap<u32, i32>) -> i32 { g(m.get_mut(&1)) }", "a `&mut i32` from `std::collections::HashMap::<K, V, S, A>::get_mut` used as a value"],
  ["a std &mut to a number given to a closure", "pub fn f(v: &mut Vec<i32>) { v.iter_mut().for_each(|x| *x += 1) }", "a `&mut i32` from `core::slice::<impl [T]>::iter_mut` used as a value"],
  ["std &muts to numbers collected", "pub fn f(v: &mut Vec<i32>) -> usize { let rs: Vec<&mut i32> = v.iter_mut().collect(); rs.len() }", "a `&mut i32` from `core::slice::<impl [T]>::iter_mut` used as a value"],
  ["a std &mut to a number from values_mut", "use std::collections::HashMap;\npub fn f(m: &mut HashMap<u32, i32>) { for x in m.values_mut() { *x += 1; } }", "a `&mut i32` from `std::collections::HashMap::<K, V, S, A>::values_mut` used as a value"],
  ["a std &mut to a number bound and kept", "use std::collections::HashMap;\npub fn f(m: &mut HashMap<u32, i32>) { let k; match m.get_mut(&1) { Some(v) => k = v, None => return } *k += 1; }", "a `&mut i32` from a std call used as a value"],
  ["a generic &mut T to an object in a struct", "pub struct C { pub n: i32 }\npub struct H<'a, T> { pub r: &'a mut T }\nfn get<'a, T>(h: H<'a, T>) -> &'a mut T { h.r }\npub fn f(c: &mut C) { get(H { r: c }).n += 1; }", "a `&mut C` inside a generic function's parameters or result"],
  ["generic &mut Ts to objects returned in a Vec", "pub struct C { pub n: i32 }\nfn to_vec<T>(a: &mut T) -> Vec<&mut T> { vec![a] }\npub fn f(c: &mut C) -> usize { to_vec(c).len() }", "a `&mut C` inside a generic function's parameters or result"],
  ["a generic &mut T to an object in an Option", "pub struct C { pub n: i32 }\nfn set<T>(o: Option<&mut T>, v: T) { if let Some(r) = o { *r = v; } }\npub fn f(c: &mut C) { set(Some(c), C { n: 7 }); }", "a `&mut C` inside a generic function's parameters or result"],
  ["a generic &mut T to an object given to a closure", "pub struct C { pub n: i32 }\nfn apply<T, F: FnMut(&mut T)>(t: &mut T, mut f: F) { f(t); }\npub fn f() -> i32 { let mut c = C { n: 1 }; apply(&mut c, |x| x.n += 1); c.n }", "a `&mut C` inside a generic function's parameters or result"],
  ["a default method's &mut self of an object Self", "pub trait Ch: Sized { fn change(mut self) -> Self { self.set(5); self } fn set(&mut self, a: i32); }\npub struct X { pub a: i32 }\nimpl Ch for X { fn set(&mut self, a: i32) { self.a = a; } }\npub fn f() -> i32 { X { a: 1 }.change().a }", "`&mut` to a `Self`"],
  // A generic associated type of lifetimes is an associated type (ADR 0146); one of a type with a bound isn't.
  ["a generic associated type of a type with a bound", "pub trait Wrap { type Of<T>: Clone; fn wrap<T: Clone>(t: T) -> Self::Of<T>; }", "generic associated types with bounds"],
  ["an associated type's value, where a type has a destructor", "pub struct Guard;\nimpl Drop for Guard { fn drop(&mut self) {} }\npub trait Source { type Item; fn next_item(&mut self) -> Option<Self::Item>; }\npub fn drain<S: Source>(s: &mut S) -> usize { let mut n = 0; while let Some(_item) = s.next_item() { n += 1; } n }", "a value of an associated type, where a type may have a destructor"],
  ["an extern declaration of the crate's own no_mangle function", "pub mod export { #[unsafe(no_mangle)] pub extern \"C\" fn twice(t: i32) -> i32 { t * 2 } }\nunsafe extern \"C\" { fn twice(t: i32) -> i32; }\npub fn f() -> i32 { unsafe { twice(3) } }", "an `extern` declaration of this crate's own `#[no_mangle]` function"],
  ["a generic constant", "#![feature(generic_const_items)]\n#![allow(incomplete_features)]\npub trait Sizes { const SIZE<T>: usize; }", "generic constants"],
  ["a type constant, refused not crashed", "#![feature(min_generic_const_args)]\n#![allow(incomplete_features)]\npub trait Foo { type const N: usize; }\npub struct Bar;\nimpl Foo for Bar { type const N: usize = 3; }", "type constants"],
  ["a generic impl's constant of its parameters", "pub trait Size { const SIZE: usize; }\npub struct W<T>(pub T);\nimpl<T> Size for W<T> { const SIZE: usize = std::mem::size_of::<T>(); }\nfn size<S: Size>() -> usize { S::SIZE }\npub fn f() -> usize { size::<W<u8>>() }", "a generic impl's constant of its parameters"],
  ["a generic const expression", "#![feature(generic_const_exprs)]\n#![allow(incomplete_features)]\nfn count<const N: usize>() -> usize { N }\nfn one_more<const N: usize>() -> usize where [(); N + 1]: { count::<{ N + 1 }>() }\npub fn f() -> usize { one_more::<2>() }", "this const argument"],
  ["an externally implementable item", "#![feature(extern_item_impls)]\n#[eii(hello)]\nstatic HELLO: u64;\n#[hello]\nstatic HELLO_IMPL: u64 = 5;\npub fn f() -> u64 { HELLO }", "externally implementable items"],
  ["a generic trait method, where a type has a destructor", "pub struct Guard;\nimpl Drop for Guard { fn drop(&mut self) {} }\npub trait Keep { fn keep<T>(&self, t: T) -> usize; }", "generic trait methods, where a type may have a destructor"],
  // A generic iterator lent as `&mut` is the lender's, which must know where it is (ADR 0071): a field's doesn't.
  // A trait's method is called through a dictionary, whose `&mut self` is a handle (ADR 0099).
  ["a &mut to a generic iterator in a trait's method", 'pub trait Step { fn step(&mut self) -> Option<u32>; }\nimpl<I: Iterator<Item = u32>> Step for I { fn step(&mut self) -> Option<u32> { self.next() } }', "of a trait's method"],
  ["lending an iterator kept in a field", 'pub struct P { pub v: std::vec::IntoIter<u32> }\nfn first<I: Iterator<Item = u32>>(it: &mut I) -> Option<u32> { it.next() }\npub fn f(p: &mut P) -> Option<u32> { first(&mut p.v) }', "lending an iterator that isn't a local"],
  ["ref mut through a reference variable, replaced whole", 'pub struct P { pub x: u32 }\n#[allow(unused_mut)] pub fn f() -> u32 { let mut a = P { x: 1 }; let mut cur = &mut a; match *cur { ref mut n => *n = P { x: 2 } } a.x }', "assigning a whole value through a `&mut`"],
  ["{:.2e}", 'pub fn f(x: f64) -> String { format!("{:.2e}", x) }', "`{:.2e}` and the like"],
  ["malformed import", '#[rust_js::import("./style.css")]\nconst _: () = ();\npub fn f() {}', "write it"],
  ["malformed binding", '#[rust_js::link_name(123)] pub fn f() {}', "a binding needs"],
  ["handwritten JSX binding", '#[rust_js::link_name = "<div>"] fn div(a: i32, b: i32) -> i32 { unreachable!() }\npub fn f() -> i32 { div(1, 2) }', "element builders are compiler-only"],
  ["#[serde(with)]", 'mod m { pub fn serialize<S: serde::Serializer>(v: &u32, s: S) -> Result<S::Ok, S::Error> { s.serialize_u32(*v) } }\n#[derive(serde::Serialize)] pub struct W { #[serde(with = "m")] pub x: u32 }\npub fn f(w: &W) -> String { serde_json::to_string(w).unwrap() }', "`#[serde(with)]`", "serde"],
  ["#[serde(serialize_with)]", 'fn s<S: serde::Serializer>(v: &u32, s: S) -> Result<S::Ok, S::Error> { s.serialize_u32(*v) }\n#[derive(serde::Serialize)] pub struct W { #[serde(serialize_with = "s")] pub x: u32 }\npub fn f(w: &W) -> String { serde_json::to_string(w).unwrap() }', "`#[serde(serialize_with)]`", "serde"],
  ["#[serde(deserialize_with)]", 'fn d<\'de, D: serde::Deserializer<\'de>>(d: D) -> Result<u32, D::Error> { <u32 as serde::Deserialize>::deserialize(d) }\n#[derive(serde::Deserialize)] pub struct W { #[serde(deserialize_with = "d")] pub x: u32 }\npub fn f(s: &str) -> bool { serde_json::from_str::<W>(s).is_ok() }', "`#[serde(deserialize_with)]`", "serde"],
  ["reading a u128", 'pub fn f(s: &str) -> bool { serde_json::from_str::<u128>(s).is_ok() }', "does not support", "serde"],
  ["reading a BinaryHeap", 'pub fn f(s: &str) -> bool { serde_json::from_str::<std::collections::BinaryHeap<u32>>(s).is_ok() }', "deserializing", "serde"],
  ["unsupported Value method", 'pub fn f(v: &serde_json::Value) -> bool { v.pointer("/name").is_some() }', "`Value::pointer`", "serde"],
  ["assigning to a Value's key", 'pub fn f() -> String { let mut v = serde_json::json!({}); v["k"] = serde_json::json!(1); v.to_string() }', "assigning to this place", "serde"],
  // A JSON object's keys are strings: serde_json refuses a struct's (ADR 0121).
  ["a map keyed by a struct in JSON", '#[derive(PartialEq, Eq, Hash, serde::Serialize)] pub struct P { pub x: u32 }\npub fn f(m: &std::collections::HashMap<P, u32>) -> String { serde_json::to_string(m).unwrap() }', "a map key of", "serde"],
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

// rust-js is a stable release's rustc (ADR 0109): a crate's own
// `#![feature]` is refused, as that release refuses it, unless
// `RUSTC_BOOTSTRAP=1`, as for rustc. rust-js has no feature of its own to
// turn on (ADR 0110), so one it once used is refused too.
test("a crate's own #![feature] is refused, as on a stable release", () => {
  const dir = fixture("stable-feature");
  const compile = (source: string, bootstrap?: string) => {
    const input = join(dir, "lib.rs");
    writeFileSync(input, source);
    const { RUSTC_BOOTSTRAP: _, ...env } = process.env;
    return Bun.spawnSync([compiler, input, "-o", join(dir, "lib.js")], { env: bootstrap ? { ...env, RUSTC_BOOTSTRAP: bootstrap } : env });
  };
  const never = "#![feature(never_type)]\npub fn f() -> u32 { 1 }\n";
  const refused = compile(never);
  expect(refused.exitCode).not.toBe(0);
  expect(refused.stderr.toString()).toContain("error[E0554]: `#![feature]` may not be used on the stable release channel");
  expect(refused.stderr.toString()).toContain("lib.rs:1:1");
  expect(compile(never, "1").exitCode).toBe(0);
  expect(compile("#![feature(register_tool)]\n#![register_tool(rust_js)]\npub fn f() -> u32 { 1 }\n").exitCode).not.toBe(0);
  expect(compile("pub fn f() -> u32 { 1 }\n").exitCode).toBe(0);
});

// rust-js's syntax is stable Rust's (ADR 0110): what it turns on for itself
// isn't a program's to use. Each of these is refused as stable 1.98.1
// refuses it, which its rustc confirms; `rust_js`'s attributes are rust-js's.
test("a program can use no unstable feature rust-js's syntax once used", () => {
  const dir = fixture("stable-syntax");
  const { RUSTC_BOOTSTRAP: _, ...env } = process.env;
  const input = join(dir, "lib.rs");
  const verdicts = (source: string) => {
    writeFileSync(input, source);
    const js = Bun.spawnSync([compiler, input, "-o", join(dir, "lib.js")], { env });
    const rustc = Bun.spawnSync(["rustc", "--edition=2024", "--crate-type=lib", "--emit=metadata", input, "-o", join(dir, "lib.rmeta")], { env, cwd: root });
    const first = (text: string) => text.split("\n").find((line) => line.startsWith("error")) ?? "";
    return { rustJs: js.exitCode === 0, rustc: rustc.exitCode === 0, stderr: js.stderr.toString(), same: first(js.stderr.toString()) === first(rustc.stderr.toString()) };
  };
  for (const [feature, source] of [
    ["stmt_expr_attributes", "pub fn f() -> u32 { let x = #[allow(unused)] 5; x }\n"],
    ["decl_macro", "macro m() {}\npub fn f() {}\n"],
    ["register_tool", "#![register_tool(foo)]\npub fn f() {}\n"],
    ["custom_inner_attributes", "mod m {\n    #![rustfmt::skip]\n}\npub fn f() {}\n"],
  ]) {
    const v = verdicts(source);
    expect([feature, v.rustc]).toEqual([feature, false]);
    expect([feature, v.rustJs]).toEqual([feature, false]);
    // The same error, rustc's own: a stable release names no feature to turn on.
    expect([feature, v.same]).toEqual([feature, true]);
  }
  // A binding's attribute is rust-js's own tool's, which a program needs no
  // feature to write.
  const binding = verdicts('#[rust_js::link_name = "Date.now"]\npub fn now() -> f64 {\n    unreachable!()\n}\n');
  expect([binding.rustJs, binding.stderr]).toEqual([true, ""]);
});
