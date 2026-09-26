# 0034. String methods are JS's; a `char` is a one-character string; `format!` is `+`

Status: Accepted. Extends [0023](0023-strings-references-shared-state.md).

## Context

A `String` or `&str` is a JS string (ADR 0023), but rust-js knew only a few
of their methods: `trim`, `is_empty`, `==`, `+`, `to_string`. Every part of
the playground left to port works with paths and messages: `split("/")`,
`ends_with(".rs")`, `replace`, `join`, and template strings. `format!` and
`char` were errors.

Rust and JS agree on what most string operations mean, but not on how to
count: Rust's `len()` and `&s[a..b]` count UTF-8 bytes, and JS's `length` and
`slice` count UTF-16 units. ReScript's strings are JS's, with JS's counting.

## Decision

**A method that means the same in both is the JS one:**

| Rust | JS |
|---|---|
| `s.starts_with(p)`, `s.ends_with(p)`, `s.contains(p)` | `s.startsWith(p)`, `s.endsWith(p)`, `s.includes(p)` |
| `s.replace(a, b)` | `s.replaceAll(a, b)` |
| `s.to_uppercase()`, `s.to_lowercase()` | `s.toUpperCase()`, `s.toLowerCase()` |
| `s.trim_start()`, `s.trim_end()`, `s.repeat(n)` | `s.trimStart()`, `s.trimEnd()`, `s.repeat(n)` |
| `s.strip_prefix(p)`, `s.strip_suffix(p)` | `$stripPrefix(s, p)`, `$stripSuffix(s, p)`: an `Option` (ADR 0030) |
| `s.split_once(p)`, `s.rsplit_once(p)` | `$splitOnce(s, p)`, `$rsplitOnce(s, p)`: an `Option` of the two sides |
| `s.split(p)` | `s.split(p)`, an array: for `for`, `collect()`, `last()` (`.at(-1)`), `count()` |
| `parts.join(sep)` | `parts.join(sep)` |
| `s.push_str(t)`, `s.push(c)` | `s = s + t` |
| `s.clone()` | `s` |

- **A pattern must be a string or a `char`.** A closure or a set of `char`s is
  an error, for now.
- **`push_str` and `push` assign**: JS strings don't change, so the variable
  gets a new string. The string must be in a place rust-js can assign to,
  a variable or a field, not behind a `&mut String` parameter.

**A `char` is a string of one character**: `'/'` is `"/"`. It goes wherever
a string pattern does (`split('/')`), and `==` and `to_string` work as on
strings.

**A string literal in a `match` is `===`**, since JS compares strings by
content: `"abc" | "stats.rs" =>` tests `s === "abc" || s === "stats.rs"`.
In `Some("ab")`, `top === "ab"` already rules out `None`, so there's no
`!= null`.

**`format!` is the pieces joined with `+`**, from the template rustc builds,
as `panic!` already was (ADR 0026):

```rust
format!("{name}: {} item{}", n, if n == 1 { "" } else { "s" })
```

```js
name + ": " + String(n) + " item" + (n === 1 ? "" : "s")
```

`format_args!` keeps its values in two `let`s of its own, a tuple and then an
array. rust-js recognizes the whole block, as it does `?`, and writes each
value where the template shows it: `return t.toFixed(0) + " ms";`. The
values are operands like any others, so a call in one doesn't put the calls
before it in `const`s. Some values still go in a `const` first, which is
then used in the string:

- **A value the template shows twice**, unless it's a variable or a
  constant, so that it runs once. So is one `{:?}` shows by its parts, as
  it does an `Option`, a `Result` or a tuple: `const arg = first_dup(s);`
  and then `arg == null ? "None" : ..`. Values before it that have effects
  go in `const`s first, in their order.
- **Values shown in another order than they're written,** when one of them
  has effects: `format!("{1} {0}", tick(&c), tick(&c))`. Each value that
  isn't a place goes in a `const`, in the order Rust runs them.

  Rust puts named arguments like `{name}` after the others, but reordering
  values that have no effects changes nothing. A place is safe too: every
  value is borrowed until the string is made, so none can change another.

**Byte counts are an error**: `len()` of a string, and indexing or slicing
one by a range, say that JS counts differently. `is_empty()` works.

## Why

- **The JS reads like hand-written JS**, with the names every JS programmer
  knows, and no helper for what JS already does.
- **It's what the playground needs**: its paths, file names and messages.
- **Refusing byte counts is honest.** Mapping `len()` to `length` would agree
  for ASCII and quietly disagree otherwise. An error says so where it matters.

## Alternatives

- **Counting bytes in JS** (`new TextEncoder().encode(s).length`): faithful,
  but slow, and slicing by bytes would have to encode and decode each time.
- **A `char` as its code point, a number**, as ReScript and Scala.js do: good
  for arithmetic on characters, but then `split('/')` needs a conversion, and
  a `char` in a message prints as a number.
- **`s.split(p)` as a real iterator** (a JS generator): lazy, like Rust's, but
  `for`, `collect` and `last` are what programs do with it, and an array
  serves them all.

## Consequences

- JS and Rust disagree at the edges: `trim_*` uses JS's whitespace, which is
  nearly Rust's, and comparing `char`s with `<` compares UTF-16 units, which
  orders a few characters above U+FFFF differently than Rust does.
- `{}` takes strings, `char`s, integers, `bool`s and floats, and formatting
  options (`{:>8}`, `{:.2}`, `{:#x}`) are ADR 0058's.
- `lines`, `split_whitespace`, `parse` and `char`'s questions are ADR 0063's.
- Not yet: `find`, `chars`, `char_indices`, `split` beyond those uses, and
  patterns that are closures.
