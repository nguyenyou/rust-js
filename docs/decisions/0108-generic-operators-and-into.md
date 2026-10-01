# 0108. Operators and `Into` in generic code are dictionaries

Status: Accepted. Extends [0049](0049-traits-and-generics.md),
[0063](0063-text.md) and [0064](0064-numbers.md).

## Context

`a + b` of a `u32` is `(a + b) >>> 0`, and of a `Vec2` its impl's
`vec2Add_add(a, b)` (ADR 0064). Of a `T: Add` it was an error: std's
operators had no dictionaries. Nor had `Into`, so the idiom for a function
that takes anything that makes a `String`, `name: impl Into<String>`, was
an error too. Both are everyday generic Rust.

## Decision

**A bound on an operator, `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg` or `Not`,
the bitwise `BitAnd`, `BitOr`, `BitXor`, `Shl` or `Shr`, or on `Into`, is a
dictionary, as `Clone`'s is:** `total(values, zero, TAdd)`, and `a + b` of
a `T` is `TAdd.add(a, b)`.

- **A number's is its own operator**, as `a + b` of one is: `{ add: (a, b)
  => (a + b) >>> 0 }` of a `u32`, `(a + b) & 255` of a `u8`, `$bigDiv` of an
  `i64`'s `/`. So it wraps, and divides, as it does outside generic code.
  One with the crate's type on either side, `impl Mul<V2> for f64`, is the
  crate's impl. A shift's amount may be of another type, `Shl<u64>` of a
  `u32`: a BigInt amount is made a number, `a << Number(b & 63n)`, as JS
  won't shift a number by one.
- **Of a number and a reference to one, `a & &b`, as of two numbers:** std's
  impl for references, as an iterator's `fold(0, |all, bit| all | bit)`
  calls, and `c >>= &1u8`, are the number's own operator.
- **The crate's type's is its impl's**, `vec2Add()`, `{ add: vec2Add_add }`,
  as any trait's dictionary is (ADR 0049).
- **`Into<U>`'s is std's conversion**, which is often the value itself,
  `{ into: (s) => s }` of a `&str` to a `String`, or `BigInt(n)` of a `u8` to
  an `i64` (ADRs 0063 and 0086); **or the crate's `From`**, `{ into:
  nameFromStr_from }` of `impl From<&str> for Name`; or, of a `T` to itself,
  the value.
- **Where the types are known, nothing changes:** `"paren".into()` is
  `"paren"`, and `v + w` `vec2Add_add(v, w)`. A dictionary is for generic
  code.
- **`name: impl Into<String>`'s evidence is `IntoString`**: rustc names the
  parameter as it's written, `impl Into<String>`, which is no word of a name.
- **A library's operator impls' dictionaries are exported**, as its other
  traits' are, for its consumers' generic code (ADR 0100).
- **Not `From` as a bound**, `T::from(1)` of a `T: From<u32>`: its impls
  are the crate's `?` conversions, and aren't dictionaries (ADR 0052).

## Why

- **The number's own operator** is what makes `total(&[200u8, 50], 5)` wrap
  as `200u8 + 50 + 5` does: the dictionary's entry is the JS `a + b` of that
  type makes, not a JS `+` of its own.

## Consequences

- **Generic operators are in** (`generic_operators`): each operator, of
  integers of three widths, `i64`, `f64` and `bool`, the crate's types, a
  right side of another type, and a number's impl of the crate's.
- **Generic `Into` is in** (`generic_into`): `impl Into<String>` of a `&str`,
  a `String` and a `char`, the crate's `From`, of a type to itself, and
  widening to `i64`.
