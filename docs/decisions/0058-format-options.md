# 0058. Format options, where Rust applies them

Status: Accepted. Extends [0034](0034-strings-and-chars.md) and [0054](0054-display.md).

## Context

`format!` was string concatenation with no options (ADR 0034). `{:>8}`,
`{:.2}`, `{:05}`, `{:#x}` and the like were errors, although tables and
numbers in output need them all the time.

Rust decides for itself which values an option applies to, and it's not
all of them. Numbers, strings, `char`s and `bool`s pad. A `fmt` that
writes with `write!` ignores width and alignment, and so does a `str`'s
`{:?}`. Rust also rounds differently from JS: `format!("{:.0}", 2.5)` is
`2`, rounding an exact tie to even, where `(2.5).toFixed(0)` is `"3"`.

## Decision

**Each placeholder's options, read from the template rustc builds, are
applied as Rust applies them:**

| Rust | JS |
|---|---|
| `{n:>6}`, `{n:<6}` of a number | `String(n).padStart(6)`, `padEnd(6)` |
| `{s:^9}`, `{s:*>9}` of a string | `$pad(s, 9, "^")`, `$pad(s, 9, ">", "*")` |
| `{n:05}`, `{n:+}` | `$zeroPad(String(n), 5)`, `$plus(String(n))` |
| `{x:.2}`, `{x:8.3}` of an `f64` | `$toFixed(x, 2)`, `$toFixed(x, 3).padStart(8)` |
| `{s:.3}` of a string | `Array.from(s).slice(0, 3).join("")` |
| `{n:#x}`, `{n:X}`, `{n:#010b}`, `{n:o}` | `"0x" + (n >>> 0).toString(16)`, … |
| `{n:>w$}`, `{x:.*}` | the width or precision is that argument |
| `{x:?}` of an `f64` | `$debugF64(x)`: `1.0`, `1e16`, `1e-5` |

- **What pads, as in Rust:**
  - numbers are right-aligned by default, and strings, `char`s and `bool`s
    left-aligned;
  - a `fmt` of the crate's own that writes with `write!`, and a `str`'s
    `{:?}`, ignore width and alignment.
- **Numbers pad with JS's own `padStart`**, since their digits are ASCII.
  A string pads with `$pad`, which counts `char`s as Rust does, where JS
  would count UTF-16 units.
- **`$toFixed` is exact.** It takes the `f64`'s bits as a fraction and
  rounds a tie to even, as Rust does. JS's `toFixed` rounds a tie up, and
  past 1e21 it switches to an exponent.
- **Negative numbers in other bases are their bits:** `{:x}` of `-1i32` is
  `ffffffff`, from `n >>> 0` (or `& 0xff` and `& 0xffff` for `i8` and
  `i16`).
- **Still errors:** `{:e}`, `{:x?}`, a precision for a value it doesn't
  apply to, and options in a `panic!` message.
- **A width or a sign is an error where it wouldn't reach the `fmt` that
  uses it.** Rust gives a placeholder's options to the value's `fmt`: a
  `{:?}` of an `Option`, a tuple, a `Vec` or a derived `Debug` gives them
  to each part, `{:5?}` of `Some(1)` is `Some(    1)`; a `fmt` that hands
  its `Formatter` on, `self.0.fmt(f)`, gives them to that one; and a
  generic `T`'s or a `dyn`'s `fmt` is a dictionary's, which only it
  knows. rust-js pads a number, a `bool` and a string itself, and a `fmt`
  of the crate's own that only writes, with `write!`, ignores them, as
  Rust's does; any other is an error until the options reach it. Each was
  dropped, so `[{:>8}]` of a `Meters(1.5)` that hands its `Formatter` to
  its `f64` showed `[1.5]` where Rust shows `[     1.5]`. (Amended twice:
  they were dropped, then only a generic `T`'s and a `dyn`'s were an error.)

## Why

- **Checked against Rust's own output,** by differential tests:
  - widths around multi-byte text (`héllo`, `日本語テキスト`);
  - negative numbers in every base;
  - exact ties, rounded to even.
- **The common case reads like JS:** `String(n).padStart(6)`.
- **It stays as Rust decides it,** including where Rust ignores an option,
  so a table formats the same in both.

## Alternatives

- **`toFixed` and `padStart` everywhere.** It would be shorter, but wrong on
  exact ties (0.5, 2.5, 0.25 are common), and wrong for strings with
  characters above U+FFFF.
- **One helper per placeholder, `$format(value, spec)`,** decided at run
  time. It's more general, but a call with an options object is harder to
  read than `padStart`, and the options are known when compiling.

## Consequences

- An integer literal before a `.` prints as `(5).toString(2)`. rust-js
  prints integers as their digits, so oxc can't add its usual space.
