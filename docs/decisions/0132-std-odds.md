# 0132. A `Box` from its value, `Default` of a `&str`, `type_name`, and standard streams

Status: Accepted. Extends [0023](0023-strings-references-shared-state.md) and [0054](0054-display.md).

## Context

Some small std forms were errors:

- `22.into()` and `Box::from(x)` into a `Box`, and a `Vec`'s items into a
  `Box<[T]>`.
- `Default::default()` of a `&str`, a slice, an array and `PhantomData`,
  and `PhantomData`'s `{:?}`.
- `any::type_name::<T>()` and `type_name_of_val`.
- `io::stdout()`, `io::stderr()`, their `lock()`, and `write!`, `writeln!`
  and `flush()` on them, with the `io::Result<()>` each gives.

## Decision

| Rust | JS |
|---|---|
| `let b: Box<i32> = 22.into();`, `Box::from(x)` | `22`, `x`: a `Box` is its value (ADR 0023) |
| `let s: Box<[i32]> = vec![4, 5].into();` | `[4, 5]` |
| `Default::default()` of `&str`, `&[T]`, `[u16; 3]`, `Box<str>`, `Box<[T]>` | `""`, `[]`, `[0, 0, 0]`, `""`, `[]` |
| `Default::default()` of `PhantomData<u8>`, and its `{:?}` | `undefined`, `"PhantomData<u8>"` |
| `type_name::<Vec<Option<&str>>>()` | `"alloc::vec::Vec<core::option::Option<&str>>"` |
| `let mut out = io::stdout();` | `let out = undefined;` |
| `writeln!(out, "hi {}", n).unwrap();` | `console.log(\`hi ${n}\`);` |
| `write!(out, "{} ", x)?;` | `$print(\`${x} \`);` |
| `writeln!(io::stderr(), ..)` | `console.error(..)` |
| `out.flush()?;`, `stdout.lock()` | nothing |

- **`type_name` is rustc's own name** for the type, as rustc's function
  makes it at compile time, written as a string. Of a type parameter,
  which each caller names, it's an error.
- **A standard stream holds nothing JS needs**: it's `undefined`, as a unit
  struct is. Its type says which stream it is, so a write to it is
  `print!`'s, or `eprint!`'s, and its lock is the same nothing.
- **An `io::Result<()>` is nothing**, as a `fmt::Result` is (ADR 0054): JS's
  writes to a stream don't fail, and nothing rust-js writes to does, so
  it's always `Ok`. `?` on it is the write; `unwrap()` and `expect()` are
  too. Matching on one, and an `io::Error`, are errors.

## Why

- **It's the JS a person writes**: a value for its box, `""` for an empty
  string, and `console.log` for a line.
- **It's exact**: what's written, and to which stream, is what Rust
  writes, compared with native Rust by the `std_odds` corpus case.

## Consequences

- These compile.
- A write through a generic `impl Write` parameter, `write_all` and
  `Mutex` are still errors.
