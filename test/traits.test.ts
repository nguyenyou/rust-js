import { beforeAll, expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCompiler, compiler, fixture, root, run } from "./support";

beforeAll(buildCompiler, 600_000);

const extra = `
impl Default for Circle { fn default() -> Self { Circle { r: 2.0 } } }
impl Labeled for Square {}
pub fn fresh_circle() -> f64 { fresh::<Circle>() }
pub fn fresh_number() -> f64 { fresh::<f64>() }
pub fn label() -> String { Circle { r: 1.0 }.label() }
pub fn default_label() -> String { Square(2.0).label() }
pub fn labeled<T: Labeled>(x: &T) -> f64 { x.area() }
pub fn super_bound() -> f64 { labeled(&Circle { r: 1.0 }) }
pub fn both<T: Shape, U: Shape>(x: &T, y: &U) -> f64 { x.area() + y.area() }
pub fn two_bounds() -> f64 { both(&Square(2.0), &3.0) }
pub fn upcast(x: &dyn Labeled) -> f64 { let s: &dyn Shape = x; s.area() }
pub fn upcast_square() -> f64 { upcast(&Square(3.0)) }
pub fn closure<T: Shape>(x: &T) -> f64 { (|a: &T| a.area())(x) }
pub fn function_value() -> f64 { let f = closure::<Square>; f(&Square(3.0)) }
pub fn default_function_value() -> String { let f = <Square as Shape>::name; f(&Square(3.0)) }
pub fn nested() -> f64 { total(&[vec![Square(2.0)], vec![Square(3.0)]]) }
pub fn identity<T>(x: T) -> T { x }
pub fn first<T: Copy>(xs: &[T]) -> T { xs[0] }
#[derive(Clone, Copy)] pub struct Point { pub x: i32 }
pub fn copies() -> i32 { let items = [Point { x: 2 }]; let mut p = first(&items); p.x = 5; p.x + items[0].x }
pub fn identity_value() -> i32 { identity(6) }
pub trait Compute { fn add(&self, shape: i32) -> i32; }
impl Compute for i32 { fn add(&self, shape: i32) -> i32 { *self + shape } }
pub fn dyn_argument_name(shape: i32) -> i32 { let value: &dyn Compute = &2; value.add(shape) }
fn bump(c: &std::cell::Cell<i32>) -> i32 { c.set(c.get() + 1); c.get() }
fn make(c: &std::cell::Cell<i32>) -> Box<dyn Compute> { Box::new(bump(c)) }
pub fn evaluation_order() -> i32 { let c = std::cell::Cell::new(0); let shape = 7; make(&c).add(bump(&c) + shape) + c.get() * 100 }
pub fn primitive_dyn() -> f64 { let n: Box<dyn Shape> = Box::new(3.0); n.area() }
pub fn enum_dyn() -> f64 { let b: Box<dyn Shape> = Box::new(Blob::Line(4.0)); b.area() }
pub fn nan_max() -> f64 { largest(&[Box::new(f64::NAN), Box::new(4.0)]) }
pub trait Cost { fn cost(&self) -> f64; }
impl Cost for Square { fn cost(&self) -> f64 { 2.0 } }
pub struct Holder<T>(pub T);
impl<T: Shape + Cost> Shape for Holder<T> { fn area(&self) -> f64 { self.0.area() + self.0.cost() } }
pub fn factory_bounds() -> f64 { total(&[Holder(Square(3.0))]) }
pub fn make_area<T: Shape + 'static>(value: T) -> Box<dyn Fn() -> f64> { Box::new(move || value.area()) }
pub fn captured_evidence() -> f64 { let a = make_area(Square(2.0)); let b = make_area(3.0); a() + b() }
pub trait Named<'a> { fn get(&self) -> &'a str; }
impl<'a> Named<'a> for &'a str { fn get(&self) -> &'a str { self } }
pub fn read_named<'a>(value: &dyn Named<'a>) -> &'a str { value.get() }
pub fn lifetime_dyn() -> String { read_named(&"hello").to_string() }
pub fn float_display(value: f64) -> String { format!("{}", value) }
pub fn float_label(r: f64) -> String { Circle { r }.label() }
`;

let output: string;
let module: any;
let directory: string;
beforeAll(async () => {
  directory = fixture("traits");
  const source = readFileSync(join(root, "examples/traits.rs"), "utf8") + extra;
  writeFileSync(join(directory, "lib.rs"), source);
  run([compiler, join(directory, "lib.rs"), "-o", join(directory, "lib.js")]);
  output = readFileSync(join(directory, "lib.js"), "utf8");
  module = await import(join(directory, "lib.js"));
}, 600_000);

test("concrete, generic and dyn calls match native Rust", () => {
  const names = ["demo", "fresh_circle", "fresh_number", "label", "default_label", "super_bound", "two_bounds", "upcast_square", "function_value", "default_function_value", "nested", "copies", "identity_value", "evaluation_order", "primitive_dyn", "enum_dyn", "nan_max", "factory_bounds", "captured_evidence", "lifetime_dyn"];
  writeFileSync(join(directory, "native.rs"), `#[path="lib.rs"] mod cases; fn main() { ${names.map(name => `println!("{}", cases::${name}());`).join("\n")} }`);
  run(["rustc", "--edition=2024", "-Coverflow-checks=off", "-Awarnings", join(directory, "native.rs"), "-o", join(directory, "native")]);
  const expected = run([join(directory, "native")]).trim().split("\n");
  expect(names.map(name => String(module[name]()))).toEqual(expected);
  expect(module.demo()).toBe(13.14);
  expect(module.dyn_argument_name(7)).toBe(9);
});

test("dictionaries are explicit, cached and usable from JavaScript", () => {
  expect(module.total([[1], [2]], module.squareShape())).toBe(5);
  expect(module.total([], module.squareShape())).toBe(-0);
  expect(module.total([-0], module.f64Shape())).toBe(-0);
  expect(module.fresh({ default: () => 3 }, module.f64Shape())).toBe(3);
  expect(module.squareShape()).toBe(module.squareShape());
  expect(module.vecShape(module.squareShape())).toBe(module.vecShape(module.squareShape()));
  expect(module.vecShape(module.vecShape(module.squareShape())).area([[[2]], [[3]]])).toBe(13);
  expect(module.circleLabeled().Shape()).toBe(module.circleShape());
  expect(module.largest([{ value: NaN, impl: module.f64Shape() }])).toBe(0);
  expect(output).toContain("circleShape_area(c)");
  expect(output).toMatch(/function total\(shapes, TShape\)/);
  expect(output).not.toContain("function shape_name"); // defaults are copied into dictionaries
  expect(() => module.first([], { copy: (x: unknown) => x })).toThrow("index out of bounds");
});

test("Rust f64 Display agrees with native output, including special values", () => {
  writeFileSync(join(directory, "format.rs"), `fn main() { let mut bits = 1_u64; for _ in 0..2048 { bits = bits.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); let r = f64::from_bits(bits); println!("{:016x}\\t{}\\t{}", bits, r, 3.14 * r * r); } for r in [0.0, -0.0, f64::MAX, f64::MIN_POSITIVE, f64::from_bits(1), f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e-150, 1e150, 1e21, 1e-7] { println!("{:016x}\\t{}\\t{}", r.to_bits(), r, 3.14 * r * r); } }`);
  run(["rustc", "--edition=2024", "-O", join(directory, "format.rs"), "-o", join(directory, "format")]);
  const view = new DataView(new ArrayBuffer(8));
  for (const line of run([join(directory, "format")]).trim().split("\n")) {
    const [bits, expected, area] = line.split("\t");
    view.setBigUint64(0, BigInt("0x" + bits));
    expect(module.float_display(view.getFloat64(0))).toBe(expected);
    expect(module.float_label(view.getFloat64(0))).toBe("circle of area " + area);
  }
});

test("an empty trait implementation needs no function body", async () => {
  const dir = fixture("marker-trait");
  writeFileSync(join(dir, "lib.rs"), "pub trait Marker {} impl Marker for u32 {}");
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
  const m = await import(join(dir, "lib.js"));
  expect(m.u32Marker()).toEqual({});
  expect(m.u32Marker()).toBe(m.u32Marker());
});

test("trait method expressions keep their Rust source locations", async () => {
  const { decodeMappings, lookup } = await import("./sourcemap");
  const map = JSON.parse(readFileSync(join(directory, "lib.js.map"), "utf8"));
  const segments = decodeMappings(map.mappings);
  const lines = output.split("\n");
  const text = "3.14 * circle.r * circle.r";
  const line = lines.findIndex(value => value.includes(text));
  expect(line).toBeGreaterThanOrEqual(0);
  const hit = lookup(segments, line, lines[line].indexOf(text));
  const source = readFileSync(join(directory, "lib.rs"), "utf8").split("\n");
  expect(hit && source[hit.srcLine].slice(hit.srcCol).startsWith("3.14 * self.r * self.r")).toBe(true);
});

test("impls and default bodies retain their defining modules across cycles", async () => {
  const dir = fixture("trait-modules");
  writeFileSync(join(dir, "lib.rs"), `pub mod contracts; pub mod implementations; pub mod types;
    pub fn result() -> f64 { implementations::initial() + contracts::area(&types::Point { x: 3.0 }) }
    pub fn label() -> String { contracts::Shape::name(&types::Point { x: 3.0 }) }`);
  writeFileSync(join(dir, "contracts.rs"), `fn default_name() -> String { "from trait module".to_string() }
    #[rust_js::link_name = "./support.js#suffix"] fn suffix() -> String { unreachable!() }
    pub trait Shape { fn area(&self) -> f64; fn name(&self) -> String { default_name() + &suffix() } }
    pub fn area<T: Shape>(value: &T) -> f64 { value.area() }
    thread_local! { static INITIAL: std::cell::Cell<f64> = std::cell::Cell::new(super::implementations::read()); }
    pub fn initial() -> f64 { INITIAL.get() }`);
  writeFileSync(join(dir, "types.rs"), `pub struct Point { pub x: f64 }`);
  writeFileSync(join(dir, "implementations.rs"), `impl super::contracts::Shape for super::types::Point { fn area(&self) -> f64 { self.x } }
    pub fn read() -> f64 { super::contracts::area(&super::types::Point { x: 2.0 }) }
    pub fn initial() -> f64 { super::contracts::initial() }`);
  writeFileSync(join(dir, "support.js"), 'export function suffix() { return " from JS"; }');
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
  const m = await import(join(dir, "lib.js"));
  expect(m.result()).toBe(5);
  expect(m.label()).toBe("from trait module from JS");
  const impl = await import(join(dir, "implementations.js"));
  expect(impl.pointShape()).toBe(impl.pointShape());
});

test("copied JSX defaults select the implementation module's JSX extension", () => {
  const dir = fixture("trait-jsx");
  writeFileSync(join(dir, "lib.rs"), `pub mod contracts {
    #[rust_js::link_name = "<div>"] fn el() -> i32 { unreachable!() }
    pub trait View { fn render(&self) -> i32 { el() } }
  }
  pub mod implementations { pub struct Page; impl super::contracts::View for Page {} }
  pub fn render() -> i32 { contracts::View::render(&implementations::Page) }`);
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
  expect(readFileSync(join(dir, "implementations.jsx"), "utf8")).toContain("<div");
  expect(readFileSync(join(dir, "lib.js"), "utf8")).toContain('./implementations.jsx');
});

for (const [name, source, diagnostic] of [
  ["mutable dyn receiver", `trait Reset { fn reset(&mut self); } pub fn f(x: &mut dyn Reset) { x.reset(); }`, "does not support"],
  ["generic trait", `pub trait Convert<T> { fn convert(&self) -> T; }`, "generic trait parameters"],
  ["generic method", `pub trait Shape { fn f<T>(&self, value: T); }`, "generic trait methods"],
  ["associated type", `pub trait Source { type Item; }`, "associated types"],
  ["generic Option", `pub fn f<T>(x: T) -> Option<T> { Some(x) }`, "does not support"],
  ["const generic", `pub fn f<const N: usize>() -> usize { N }`, "const generics"],
  ["Drop", `pub struct Resource; impl Drop for Resource { fn drop(&mut self) {} }`, "user implementations"],
  ["colliding methods", `#![rust_js::camel_case] pub trait T { fn first_name(&self); fn firstName(&self); }`, "dictionary names collide"],
  ["reserved method", `pub trait T { fn __proto__(&self); }`, "reserved"],
]) {
  test(`${name} produces a diagnostic instead of incomplete JS`, () => {
    const dir = fixture("trait-rejection");
    writeFileSync(join(dir, "lib.rs"), source);
    const result = Bun.spawnSync([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js")]);
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain(diagnostic);
    expect(result.stderr.toString()).toContain("lib.rs:");
    expect(result.stderr.toString()).not.toContain("panicked");
  });
}
