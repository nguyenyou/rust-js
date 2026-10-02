# 0136. Common std methods: `take`, `cmp::max`, ASCII case, and `Debug`'s builders

Status: Accepted. Extends [0054](0054-display.md), [0060](0060-debug.md) and [0074](0074-mut-boxes.md).

## Context

Some std methods that ordinary Rust calls all the time were errors:

- an `Option`'s `take()` and `replace(v)`, and `mem::take(&mut x)`;
- `cmp::min(a, b)` and `cmp::max(a, b)`, as functions;
- `to_ascii_lowercase()`, `to_ascii_uppercase()` and `eq_ignore_ascii_case`
  of a `str`;
- an iterator's `inspect(f)`, a `Result`'s `as_ref()`, a `Cell`'s and a
  `RefCell`'s `into_inner()`, a `Vec`'s and a `VecDeque`'s `append`, and a
  float's `exp2()`, `exp_m1()` and `ln_1p()`;
- a hand-written `Debug`'s builders: `f.debug_struct("P").field("x",
  &self.x).finish()`, and `debug_tuple`, `debug_list`, `debug_set` and
  `debug_map`.

## Decision

| Rust | JS |
|---|---|
| `let t = o.take();` | `const old = o; o = undefined; const t = old;` |
| `o.replace(5)`, `mem::take(&mut v)` | as `mem::replace` with `Some(5)`, and `v`'s default (ADR 0098) |
| `cmp::max(a, b)` | `a.max(b)`'s: `Math.max(a, b)`, or `Ord::max`'s |
| `s.to_ascii_lowercase()` | `$asciiCase(s)`: ASCII's letters only |
| `a.eq_ignore_ascii_case(b)` | `$asciiCase(a) === $asciiCase(b)` |
| `iter.inspect(f)` | `iter.map((item) => { f(item); return item; })` |
| `r.as_ref()` of a `Result` | `r`: a reference is the value (ADR 0023) |
| `cell.into_inner()` | `cell.value` |
| `v.append(&mut other)` | `$append(v, other)`, which empties `other` |
| `x.exp2()`, `x.exp_m1()`, `x.ln_1p()` | `2 ** x`, `Math.expm1(x)`, `Math.log1p(x)` |
| `f.debug_struct("P").field("x", &self.x).finish()` | `` `P { x: ${p.x} }` ``, written as one string |
| `f.debug_list().entry(&0).entries(v.iter()).finish()` | `` `[${["0"].concat(Array.from(v, ..)).join(", ")}]` `` |

- **`take` and `replace` write the place**, as `mem::replace` does, through
  a box for a `&mut` to one (ADR 0074), and give back the old value, boxed
  already where a generic one is (ADR 0051). Of a value with a destructor,
  which the old one would need dropping, they're still errors.
- **`cmp::max` is `Ord::max`**: of numbers, `Math.max`, and of anything else,
  the order rust-js compares it by, as the method's is. Equal, it's the
  second, as Rust's is.
- **ASCII case changes only ASCII's letters**, as Rust's does: `Ü` stays.
- **A `Debug` builder's chain is one string**, what a derived `Debug`
  writes: its `&dyn Debug` arguments are the strings they show already (ADR
  0060), and `entries` an array of them, joined with the rest. A builder
  kept in a variable and written to in turn, and `{:#?}`, are still errors.

## Why

- **It's the JS a person writes**: a write and a read, a string of the
  fields, `2 ** x`.
- **It's exact**: what each gives and changes is Rust's, compared with native
  Rust by the `std_methods` corpus case.

## Consequences

- These compile.
- `by_ref()`, which shares an iterator's place with a later use, is still an
  error.
