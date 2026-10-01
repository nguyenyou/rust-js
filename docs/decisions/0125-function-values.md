# 0125. A constructor, or a closure as a `fn`, is a JS function

Status: Accepted. Extends [0030](0030-option.md) and [0033](0033-enums-with-fields.md).

## Context

A function of the crate's was a value already, its JS name, and a std
function of one argument, `.map(str::trim)`, an arrow doing what its call
does. But `.map(Some)`, `.map(Meters)`, `.map(Shape::Circle)`,
`.map(i32::abs)`, `let make = Shape::Rect;` and `let f: fn(i32) -> i32 =
|x| x + 1;` were errors: taking a constructor or a closure where a
function goes is how Rust code maps, and rustc's tests stopped there most
often of the expressions they used (ADR 0089).

## Decision

**A constructor taken as a value is an arrow of its fields that makes what
its call makes; a closure as a `fn` is the closure:**

| Rust | JS |
|---|---|
| `.map(Some)` | `.map((value) => value)` (ADR 0030) |
| `.map(Ok)` | `.map((value) => ({ TAG: "Ok", _0: value }))` |
| `.map(Meters)` of `struct Meters(f64)` | `.map((value) => [value])` (ADR 0020) |
| `let make = Shape::Rect;` | `const make = (_0, _1) => ({ TAG: "Rect", _0, _1 });` |
| `let f: fn(i32) -> i32 = \|x\| x + 1;` | `const f = (x) => (x + 1) \| 0;` |
| `.map(i32::abs)` | `.map((n) => Math.abs(n) \| 0)` |

- **The arrow's parameters are `value`**, of one field, **or `_0`, `_1`**,
  a variant's own names for them: it reads only those, so they can't take
  a name the code around it has.
- **`Some` of a type that could look like `None` is boxed**, as `Some(x)`
  is (ADR 0051), and one that can't be told from `None` is an error, as its
  call is.
- **A closure that captures nothing is a JS function already**, so its
  coercion to a `fn`, rustc's `ClosureFnPointer`, is the closure.
- **A number's method of one argument**, `i32::abs`, is what its call is,
  wrapped where its call is: `i32::MIN`'s is itself. An `f32`'s library
  function, `.map(f32::sqrt)`, is rounded to an `f32` as its call is (ADR
  0122): it wasn't, which `std_fn_value` had missed.

## Why

- **It's the JS a person writes**: `.map((value) => [value])`, and a closure
  as the function it is.
- **It's exact**: each arrow makes what the call makes, by the same
  representation.

## Consequences

- Constructors as values, closures as `fn`s and `i32::abs` compile,
  compared with native Rust by the `function_values` corpus case.
- A library function of more than one argument as a value, and an `i64`'s
  `abs`, are still errors.
