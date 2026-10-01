# 0130. An `if let` is a value anywhere, and trait and std functions are values

Status: Accepted. Extends [0125](0125-function-values.md).

## Context

Three of stable Rust's forms were errors:

- An `if let` with one `let`, used as a value anywhere but a `let`'s
  initializer: `twice(if let Some(x) = m { x } else { 0 })`, `1 + if let
  ..`, `vec![if let ..]`, `assert!(if let ..)`. Its `let`-chain form and a
  `match` were values already.
- A trait's function as a value, `.map(Area::area)` or `<Shape as
  Area>::unit`, and some of std's: `size_of::<u16>`, `.map(drop)`,
  `mem::forget`, `Option::is_some`.
- `Self` as the value of a unit struct.

## Decision

| Rust | JS |
|---|---|
| `twice(if let Some(x) = m { x } else { 0 })` | `twice(m != null ? m : 0)` |
| `!(if let Some(_) = m { true } else { false })` | `m == null` |
| `1 + if let [first, ..] = words { .. } else { 0 }` | `1 + ..`: a pattern that always matches |
| `shapes.iter().map(Area::area)` | `shapes.map(shapeArea_area)` |
| `items.map(T::twice)`, `T: Area` | `items.map((arg0) => TArea.twice(arg0))` |
| `size_of::<u16>` | `() => 2` |
| `drop`, of a value with a destructor | `(value) => { loudDrop_drop(value); }` |
| `mem::forget` | `(value) => {}` |
| `Option::is_some` | `(x) => x != null` |
| `Self`, of `struct Marker;` | `undefined`, as `Marker` is |

- **An `if let` as a value is `test ? then : else`** where its pattern
  names its variables' places, as an `if let`'s does, and needs no
  `const`s for them: else it's statements, and a `tmp` read after them.
  `{ true } else { false }` is the test itself, and `{ false } else { true }`
  its negation.
- **A trait's function as a value is an arrow calling it**, as its call
  is, or the function itself where that's all the arrow does. A
  dictionary's method stays in its arrow, so it's called on its dictionary.
- **`drop` as a value runs what dropping its value runs** (ADR 0098), and
  `forget` nothing.

## Why

- **It's the JS a person writes**: a conditional for a value, and a
  function by name.
- **It's exact**: the test, the binding, and the order things run in are
  an `if let` statement's, and a function value runs what its call does.

## Consequences

- These compile, compared with native Rust by the `expression_values`
  corpus case.
- Raw borrows, `&raw const x`, are still errors.
