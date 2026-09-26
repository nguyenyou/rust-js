# 0070. A std function taken as a value is an arrow

Status: Accepted. Extends [0034](0034-strings-and-chars.md), [0063](0063-text.md) and [0064](0064-numbers.md).

## Context

A program that parses rows of text wrote `line.split(',').map(str::trim)`:
a std function, passed where a closure goes. That was an error, as were a
`char`'s `to_uppercase()`, `{:?}` of a parse error, and `unwrap_err`.

## Decision

**A std function taken as a value is the arrow a closure would be**, one
parameter, doing what a call does:

| Rust | JS |
|---|---|
| `.map(str::trim)` | `.map((s) => s.trim())` |
| `.map(char::is_whitespace)` | `.map((c) => /^\p{White_Space}$/u.test(c))` |
| `.map(f64::sqrt)` | `.map((n) => Math.sqrt(n))` |
| `.map(String::from)`, `.map(ToString::to_string)` | `(s) => s`, `(n) => String(n)` |

It takes the forms a call has without its arguments: a JS method, a
`char` question, a `Math` function, a conversion that changes nothing, and
`to_string`. Other std functions as values are still errors.

- **`c.to_uppercase()` and `c.to_lowercase()`** are the `char`s JS's own
  mapping gives, `Array.from(c.toUpperCase())`. JS and Rust both use
  Unicode's unconditional mappings: `'ß'` is `SS` in both.
- **`{:?}` of a parse error** is its kind, from its message, which is what
  a parse error is (ADR 0063): `ParseIntError { kind: InvalidDigit }`.
- `r.unwrap_err()` and `r.expect_err(msg)` are `$unwrapErr(r)`, which
  panics with Rust's message on an `Ok`.
- An array whose length is its type's, read at a constant index below it,
  is read directly, `HEADERS[0]`: rustc has already checked it.

## Why

- **It's the JS a person writes:** `.map((s) => s.trim())`, as the
  closure `|s| s.trim()` already was.
- **It's Rust's answer.** The example's report, with its errors, columns
  and groups, matches native Rust's, and so do the parse errors' `{:?}`.

## Alternatives

- **A function per std item** (`const str_trim = (s) => s.trim()`), named
  once and passed by name. It would read well where one is used often, but
  most are used once, and the arrow in place says what it does.
