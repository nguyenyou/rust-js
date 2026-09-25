# 0016. Only top-level functions, for now

Status: Accepted (temporary scope limit)

## Context

A Rust crate contains more than free functions: modules, `impl` blocks and
methods, traits, constants, statics, closures.

## Decision

For the fib.js milestone, rust-js compiles **free functions at the crate root**.
Everything else is either harmless or reported:

| Item | Handling |
|---|---|
| `fn` at crate root | compiled; `pub` → `export` |
| `fn` inside a `mod` | error: functions inside modules |
| methods (`impl` fns) | error: methods |
| `const`, `static` | error: constants / statics |
| closures | error at the closure expression |
| `enum`, `struct` declarations | nothing to emit; uses are checked where they occur |

Calls work only between these compiled functions. Calling anything else
(`std`, `core`, methods) is an error that names the callee.

**Generic functions are rejected for now.** A value of type `T` fails the
supported-type check (`rust-js does not support values of type `T` yet`). The
planned design is to *erase* generics: JS is dynamically typed, so one copy
of `id<T>` can serve every `T`, unlike rustc, which makes a copy per type.

## Why

- Names in one flat namespace map one-to-one to JS function names, so there's
  no path mangling to design yet.
- Each rejected item has its own real design question. Methods need a choice
  of classes vs. free functions, consts need const evaluation, closures need
  captures. Each deserves its own record when we get there, rather than a
  rushed default.

## Consequences

- The first thing a real-world file hits is often a method or a `std` call.
  Expect those to be next.
- Erasure is easy while generic code only moves values around. Once it calls
  trait methods (`x.to_string()` on a `T: Display`), we'll need to decide
  between methods on prototypes and passing trait dictionaries. That
  decision gets its own record.
