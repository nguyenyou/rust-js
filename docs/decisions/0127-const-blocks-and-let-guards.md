# 0127. An inline `const` is its value, and an `if let` guard binds for its arm

Status: Accepted. Extends [0124](0124-or-pattern-bindings.md).

## Context

Two of stable Rust's forms were errors: an inline constant, `const {
square(7) + 1 }`, and a match arm's `if let` guard, `Some(x) if let Ok(n) =
x.parse() => ..`, with a let chain's `&&`s, which binds what it matches for
the arm.

## Decision

**An inline `const` is its value, as rustc computes it, as a named one's
is; and an `if let` guard is a let chain the arm's body goes inside:**

| Rust | JS |
|---|---|
| `const { square(7) + 1 }` | `50` |
| `const { [square(1), square(2)] }` | `[1, 4]` |
| `Some((k, v)) if let Ok(n) = v.parse::<i32>() => f(k, n),` | `if (m != null) { const n = $parseInt(..); if (n.TAG === "Ok") { return f(..); } }` |

- **A guard's `let`s, and its other parts**, are a let chain's levels, as
  an `if`'s are, its bindings in scope for the arm: the arm's body goes
  inside the innermost, and a level that doesn't hold goes past it, on to
  the later arms.
- **The arm is left when its body ends**, as one with a guard of statements
  is: a `break` out of the arms, unless the body returns already.
- **A guard of a name bound at a choice of `|` places is an error**, an
  `if let` one too (ADR 0124).
- **`matches!` with an `if let` guard** is still an error.

## Why

- **It's the JS a person writes**: the value, written once, and an `if`
  inside the arm's test for each `let`.
- **It's exact**: rustc computes the constant, and each `let` of a guard runs
  only where the pattern and the guard's parts before it held, as Rust's do.
