# 0126. A byte string is its bytes, and `as_bytes()` a string's UTF-8 bytes

Status: Accepted. Extends [0063](0063-text.md).

## Context

A byte string, `b"GET"`, was an error as a value, though one as a pattern
was its bytes already; and so was `s.as_bytes()`. A byte string is a `&[u8;
N]`, which rust-js has as an array of numbers (ADR 0036), and a `str` is a
JS string, of UTF-16 units, not of the UTF-8 bytes Rust's is (ADR 0063).

## Decision

| Rust | JS |
|---|---|
| `b"GET"` | `[71, 69, 84]` |
| `b"a\xff"` | `[97, 255]`: every byte, not only ASCII |
| `s.as_bytes()` | `Array.from(new TextEncoder().encode(s))` |
| `[a, b].as_slice()` of an array | the array |

- **A byte string is its bytes**, written out, as an array of `u8`s is.
- **`as_bytes()` is a copy**, the string's UTF-8 bytes: nothing can write
  through the `&[u8]` it gives, so a copy is what a view of it would be.
  A multi-byte character is more than one byte, as in Rust: `"é"` is `[195,
  169]`.
- **An array's `as_slice()` is the array**, as a `Vec`'s is (ADR 0123).

## Why

- **It's the JS a person writes**: an array of numbers, and `TextEncoder`,
  which is how JS has a string's UTF-8 bytes.
- **It's exact**: `TextEncoder` encodes as Rust's strings are encoded.

## Consequences

- Byte strings and `as_bytes()` compile, compared with native Rust by the
  `byte_strings` corpus case.
- A C string, `c"..."`, and `str::from_utf8` are still errors.
