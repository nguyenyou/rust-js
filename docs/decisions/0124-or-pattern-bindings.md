# 0124. A `|` pattern binds each name where its alternative has it

Status: Accepted. Extends [0123](0123-slice-patterns.md).

## Context

A `|` pattern was its alternatives' tests, or'ed, and an error if it
bound anything: `Circle(r) | Sphere(r) => r * 2.0`, `(0, x) | (x, 0)`,
`let (Ok(n) | Err(n)) = r;`. Rust checks that each alternative binds the
same names, of the same types, so an arm needn't be written once for each.

## Decision

**Each alternative is tested, and binds its own names; a name each binds at
the same place is that place, and one bound elsewhere in each is the place
of the first that matched:**

| Rust | JS |
|---|---|
| `Circle(r) \| Sphere(r) => r * 2.0` | `if (s.TAG === "Circle" \|\| s.TAG === "Sphere") return s._0 * 2;` |
| `(0, x) \| (x, 0) => x` | `if (p[0] === 0 \|\| p[1] === 0) return p[0] === 0 ? p[1] : p[0];` |
| `let (Ok(n) \| Err(n)) = r;` | `r._0` |

- **The same place**, as a variant's field is in each, is compared by what
  a pattern's places are made of: names, fields, indexes, numbers.
  Anything else counts as different, and is chosen between, which is
  right either way.
- **A choice of places, the last needs no test**: the pattern matched, so
  if no alternative before it did, it did.
- **A `ref mut` bound at a choice of places is an error**: `p[0] === 0 ?
  p[1] : p[0]` can't be written through. One at the same place is that
  place, and writes it.
- **A guard of a name bound at a choice of places is an error**: Rust tries
  the guard with each alternative that matches, in turn, until it holds,
  and with several `|`s each combination, left to right. `(a, _) | (_, a)
  if a > 10` of `(3, 42)` takes the arm with `a` 42, where the first
  alternative's, 3, alone wouldn't: found by rustc's
  `issue-70413-no-unreachable-pat-and-guard.rs` and
  `search-via-bindings.rs`, which this made wrong before it was. A name
  bound at the same place is the same value in each, and its guard is
  right. Trying each in turn, as Rust does, would be the arm once for each
  combination, its body in each.
- **A pattern rustc checked always matches**, a `let`'s, a parameter's or a
  `for`'s, binds without its test, as `Ok(n) | Err(n)` has one that always
  holds: it was an error, as a test was taken for a refutable pattern.

## Why

- **It's the JS a person writes**: one condition, `a || b`, and one
  binding, or a `?:` where the alternatives put it in different places.
- **It's exact**: the first alternative that matches is Rust's, and its
  bindings are what an arm sees.
