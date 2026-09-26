# 0071. An iterator stepped through is a `$iter`, which knows where it is

Status: Accepted. Extends [0036](0036-iterators-and-sorting.md) and [0055](0055-iterator.md).

## Context

An iterator over an array is the array itself (ADR 0036): `v.iter().map(f)`
is `v.map(f)`. That works while the iterator is only handed on. But a
tokenizer steps through its source:

```rust
let mut chars = src.chars().peekable();
while let Some(&c) = chars.peek() { if c.is_ascii_digit() { chars.next(); } else { break } }
```

An array doesn't know how far along it is, so `next()`, `peekable()` and
`peek()` were errors.

## Decision

**An iterator that's stepped through is a `$iter`: its items and where it
is,** `{ items, at }`, which is a JS iterator too. That's any `Peekable`,
wherever it's kept (a lexer's field), and a local that `next()` is called
on:

| Rust | JS |
|---|---|
| `src.chars().peekable()` | `$iter(Array.from(src))` |
| `let mut it = v.iter();` (then `it.next()`) | `let it = $iter(v);` |
| `it.next()` | `$next(it)`: the item, or `undefined` at the end |
| `it.peek()`, `it.next_if(f)`, `it.next_if_eq(&x)` | `$peek(it)`, `$nextIf(it, f)`, `$nextIf(it, (item) => item === x)` |
| `chars.as_str()` | `$restStr(chars)`: what's left, still there |
| `it.copied().collect()`, `for x in it` | `$rest(it)`: what's left, which it then hasn't |

- **Anything else done with one takes what it has left,** as Rust's
  adapters do, which take the iterator.
- **`next()` of an iterator just made** is its first item:
  `v.iter().skip(2).next()` is `v.slice(2)[0]`. Of a crate's own, it's its
  impl's `next` (ADR 0055), and of a lazy one, `$next` of the JS iterator.
- **`next()` of one kept elsewhere,** a `Chars` in a field or a parameter,
  or used in a closure, is an error: it would have to be a `$iter` wherever
  it came from. A `Peekable` is one everywhere, so the error says to use it.
- **`peekable()` of a lazy iterator** is an error: its items would have to
  be worked out first, which could change what runs when, or never end.
- **Items that could look like `None`** are an error, since `$next` is
  `undefined` at the end.

### Also here

- `collect::<String>()` of `Array.from(s)` is `s`: `c.to_uppercase()`
  collected is `c.toUpperCase()`.

## Why

- **It's Rust's answer.** The example's tokenizer, with numbers, words,
  strings, comments and an unterminated string, matches native Rust's, and
  so does `pairs`, which takes two items at a time and then the rest.
- **The arrays stay arrays** where nothing steps through them: only what
  needs to know where it is pays for it.

## Alternatives

- **JS's own iterators** (`array.values()`), with a buffer for `peek`.
  They step as well, but `as_str()` and `peek()` need the items and an
  index anyway, and a `$iter` can be read in a debugger.
- **Every iterator a `$iter`.** Uniform, but `v.iter().map(f)` would lose
  its plain `v.map(f)`.
