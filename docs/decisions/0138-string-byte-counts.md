# 0138. A string's length, slices and offsets count its UTF-8 bytes

Status: Accepted. Supersedes [0034](0034-strings-and-chars.md)'s byte counts being an error.

## Context

A string is a JS string (ADR 0023), of UTF-16 units, and Rust's counts its
UTF-8 bytes: `"héllo".len()` is 6, and its `length` 5. So `len()`, `&s[a..b]`,
`find` and `char_indices` were errors (ADR 0034): `length` would agree for
ASCII and quietly disagree otherwise.

They're among what ordinary Rust does most with a string: `while s.len() <
n`, `&s[..at]` after `s.find(' ')`. Refusing them refuses most programs that
take a string apart.

## Decision

**Where Rust counts bytes, rust-js counts them, the same:**

| Rust | JS |
|---|---|
| `s.len()` | `$byteLen(s)` |
| `&s[a..b]`, `&s[a..]`, `&s[..b]`, `&s[..=b]` | `$strSlice(s, a, b)`, `$strSlice(s, a)`, `$strSlice(s, 0, b)`, `$strSlice(s, 0, b + 1)` |
| `&s[..]` | `s` |
| `s.find(p)`, `s.rfind(p)` | `$find(s, p)`, `$rfind(s, p)`: an `Option` of a byte offset |
| `s.char_indices()` | `$charIndices(s)`: `[at, c]` pairs, `at` in bytes |

- **`$byteLen` counts a string's UTF-8 bytes** from its UTF-16 units: 1, 2
  or 3 for one unit, and 4 for a pair. Nothing is encoded.
- **`$strSlice` finds the units at those bytes**, and panics as Rust does,
  with Rust's messages, in its order: out of bounds, then backwards, then
  inside a character, which it names:
  `start byte index 2 is not a char boundary; it is inside 'é' (bytes 1..3 of string)`.
- **`find` and `rfind` are JS's `indexOf` and `lastIndexOf`**, of a string
  or `char` pattern, with the units before it counted in bytes. Another
  pattern is still an error.
- **`is_empty()` is still `length === 0`**: no bytes is no units.

## Why

- **It's exact**: the same numbers, slices and panics as Rust's, for any
  string, compared with native Rust by the `string_bytes`,
  `string_slice_boundary` and `string_slice_bounds` corpus cases.
- **It reads as what it does**: a byte length, a slice by bytes, where JS's
  own `length` and `slice` would read as the same thing and not be.

## Alternatives

- **`length` and `slice`, as ReScript's strings are**: the JS a person
  writes, but a different answer for any string that isn't ASCII.
- **`new TextEncoder().encode(s).length`**: as exact, but makes the bytes to
  count them, and slicing would encode and decode.
- **Strings as arrays of UTF-8 bytes**: every count exact and fast, but
  every string a conversion at the edges, and none of JS's own methods.

## Consequences

- `len()`, slicing by a range, `find`, `rfind` and `char_indices` of a
  string compile.
- Each counts from the start of the string: a loop that slices at each
  offset walks it each time.
- `match_indices`, and `find` with a closure as its pattern, are still
  errors.
