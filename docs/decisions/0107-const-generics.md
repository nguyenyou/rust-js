# 0107. A const parameter is a value its caller gives

Status: Accepted: a function's, a type's and an impl's const parameters. A
trait's, a trait method's own, and generic const expressions are to come.
Extends [0049](0049-traits-and-generics.md) and [0106](0106-generic-traits.md).

## Context

`fn sum<const N: usize>(values: [u32; N])` has a number in its generics,
not a type. rustc's tests stop at them 75 times. Rust makes a copy of `sum`
for each `N`; rust-js makes one JS function, as it does of a type parameter,
and a type parameter's dictionary is already a value its caller gives.

## Decision

**A const parameter is a parameter of its own**, before the function's
dictionaries: `sum(values, N)`, `describe(values, N, TDebug)`.

- **Its caller gives its value**: rustc's, where it's known, `sum([1, 2, 3],
  3)`, or its own `N` in generic code, `count(N)`. A `bool` is `true`, a
  `char` a string, as a constant's are (ADR 0031).
- **`N` as a value is the parameter**, and `[x; N]` of one is `new
  Array(N).fill(x)`. A closure sees its function's.
- **A type's is its impl's methods'**, given where the type is known, from
  the receiver: `Ring.push(ring, x, 2)` of a `Ring<2>`.
- **An impl's dictionary is given them**, `ringShow(2)`: one for each value,
  cached by it in a `Map`, as its dictionaries are in a `WeakMap`, which a
  number can't key. `gridShow(2, 3)`'s cache is a `Map` of `Map`s.
- **Not yet: a trait's, `trait Foo<const N: usize>`, or a trait method's
  own**, `fn f<const N: usize>(&self)`, which its dictionary would be given
  too. In a default copied into an impl, a const argument may be the
  trait's or already the impl's, and its index can't tell which. An error.
- **Not yet: a generic const expression**, `{ N + 1 }`, rustc's incomplete
  `generic_const_exprs`, which rust-js would compute itself. An error.

## Why

- **A value is what a JS caller gives**: one function, which reads as
  written, where a copy for each `N`, `sum_3`, would read as nothing a
  person writes.
- **Before the dictionaries** is the order rustc declares them in, most
  often: `<T: Debug, const N: usize>` has its `N` in the signature's types,
  `[T; N]`, and its `Debug` only in the body.

## Consequences

- **Const generics are in** (`const_generics`): a function's, given on and
  with a dictionary after it, `[x; N]`, a type's through its methods, a
  `bool`'s and a `char`'s, a closure's, a function value's; an impl's
  dictionary for two values, a `dyn`, a std trait's impl, and one of two
  parameters; and the diagnostics test's refusals. Of the 75 rustc tests
  stopping at them, 23 pass, none giving another answer; 21 stop at a
  trait's or a trait method's own, and 13 at generic const expressions.
