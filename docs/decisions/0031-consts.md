# 0031. A `const` is the value rustc computed, under its own name

Status: Accepted.

## Context

`const` items were an error. Programs want them for URLs, sizes, limits and
fixed tables, and so does the playground once it's written in Rust. In Rust,
a `const` is worked out at compile time, and each use is a value of its own,
as if the value were written there.

Two ways to get it into JS:

```text
const SIZE: u32 = 4 * 1024;
  (a) lower its expression   ─►  const SIZE = Math.imul(4, 1024) >>> 0;   computed as the module loads
  (b) ask rustc for the value ─►  const SIZE = 4096;                      computed at compile time
```

With (a), the order matters: a `const` that uses another declared later hits
JS's temporal dead zone. Across modules it's worse, since modules may import
each other (the `modules` example has `lib ↔ stats`), and one may run before
the other has set its constants. ReScript folds constant expressions too.
Scala.js inlines its `final val` constants.

## Decision

**rustc computes the value, and the JS declares it once, under the Rust
name.** Uses refer to it by name.

```rust
pub const SIZE: u32 = 4 * 1024;
const ORIGIN: Point = Point { x: 0, y: 0 };

pub fn size_in_kb() -> u32 { SIZE / 1024 }
pub fn moved(dx: i32) -> Point { let mut p = ORIGIN; p.x += dx; p }
```

```js
export const SIZE = 4096;
const ORIGIN = { x: 0, y: 0 };

export function size_in_kb() {
  return SIZE / 1024 >>> 0;
}
export function moved(dx) {
  let p = { ...ORIGIN };
  ...
}
```

- **The value comes from rustc's const evaluation**, as a value tree, and is
  written in the shapes the other ADRs give: numbers (0011), strings,
  `bool`, fieldless enum variants as strings (0013), structs and tuples as
  objects and arrays (0020), `undefined` for `None` (0030).
- **Each use is a value of its own**, so where a type is changed in place
  somewhere in the crate, a use copies (`{ ...ORIGIN }`), as reading a Copy
  place does (ADR 0020). Otherwise the name is used as it is.
- **Constants go at the top of their module's file**, before the functions,
  and are exported when they're `pub` or another module uses them (ADR 0019).
  A `const` inside a function goes there too, next to the function. Names
  follow the usual rules: a `const URL` in a module that uses the global
  `URL` becomes `URL$1`.
- **Other crates' constants are written in place**, as values: `u32::MAX` is
  `4294967295`.
- **A divisor rustc knows** needs no check for zero: `x / SIZE` is
  `x / SIZE >>> 0`, not `$div(x, SIZE)`.

## Why

- **It's what Rust means**: the value is fixed at compile time. A constant
  that doesn't compile (say, overflow) is already an error in rustc.
- **Nothing depends on load order**: literals don't read other modules, so
  cycles between modules and declaration order don't matter.
- **The JS keeps the names**: `x > SIZE` reads better than `x > 4096`.

## Alternatives

- **Lower the expression**, as in (a): the JS would show `4 * 1024`, but
  would have to order constants, and couldn't order them across module
  cycles.
- **Write every use as its value**, as Scala.js does: the simplest
  semantics, but the names are lost, and a big table would be repeated at
  every use.

## Consequences

- The JS shows the computed value, not how it was written.
- Only what a value tree holds can be a `const`: no `String` (a heap
  pointer), no function pointers, no `dyn`. Those are an error that says so.
- Not yet: associated constants (`impl Foo { const N: u32 = 3; }`),
  `static`s, and `const` blocks.
