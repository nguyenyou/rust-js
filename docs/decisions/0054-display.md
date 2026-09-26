# 0054. `Display`: a `fmt` returns the string it writes

Status: Accepted. Extends [0034](0034-strings-and-chars.md) and [0052](0052-std-trait-impls.md).

## Context

`format!` is string concatenation (ADR 0034; a template literal since ADR 0066): `format!("{} of {}", a, b)`
is `a + " of " + b`, with numbers through `String(x)` or `$displayF64(x)`.
Only std types could appear in a `{}`. A hand-written `impl Display` was
rejected, as was `x.to_string()` of one.

Rust's `Display::fmt` doesn't return a string. It writes into a `Formatter`
and returns a `fmt::Result`:

```rust
impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}
```

Copied literally, JS would need a formatter object to pass around, and a
`Result` checked after every `write!`. The JS a person writes for this is a
function that returns a string.

## Decision

**A function that writes to a `Formatter` returns the string it writes, and
takes no formatter.** That covers any function with a `&mut Formatter`
parameter that returns `fmt::Result`: a `Display` impl's `fmt`, or a helper
of the crate's own.

```js
function pointDisplay_fmt(point) {
  return "(" + String(point.x) + ", " + String(point.y) + ")";
}
```

- **The formatter is a local string, `let f = ""`.**
  - `write!(f, ..)`, `f.write_str(s)` and `f.write_char(c)` are `f += s`.
  - Calling another function that writes, such as `stop.fmt(f)` or
    `write_loop(f, n)`, is `f += <its string>`.
  - The function returns `f`.
- **It stays short when it can:**
  - one write is `return s`;
  - one write on each way through is a `return` on each:
    `if (figure === "Dot") { return "a dot"; } else { return .. }`;
  - a first write starts the string: `let f = "[";`.
- **`fmt::Result` is nothing.** Writing to a string can't fail:
  - `Ok(())` is `undefined`, and `?` on a write does nothing;
  - `return w` is `w` done, then `return f`.
- **`{}` of a value, and `x.to_string()`, is its string:**
  - a hand-written impl is a call, `pointDisplay_fmt(p)`;
  - in generic code, `T: Display` takes a dictionary, `{ fmt }`, and it's
    `TDisplay.fmt(x)`;
  - a std type's dictionary is the function itself: `{ fmt: String }`,
    `{ fmt: $displayF64 }`.
- **Still errors:**
  - `Err(fmt::Error)`;
  - using a `fmt::Result` as a value: matching it, `is_ok()`;
  - `Formatter`'s options inside a `fmt`: `alternate()`, `width()`. A
    `{:>5}` of a value whose `fmt` writes with `write!` ignores its
    options, as in Rust (ADR 0058);
  - a `Formatter` outside a function that writes to one.
- **`s = s + t` prints as `s += t`,** everywhere, as JS writes a string built up.

## Why

- **It's the JS a person would write:** a `toString`-like function, with a
  `+=` only when the string is built in steps.
- **Nothing is lost.** The formatter only collects the output, and a write
  to a string never fails. Anything else a formatter does, like its flags,
  is an error rather than something ignored.
- **One rule covers helpers too.** A function the crate writes that takes
  the formatter gets the same treatment, so `fmt` can delegate.

## Alternatives

- **A formatter object, `{ out: "" }`, passed by reference.** This is
  closer to Rust, but every `fmt` would need a helper to call it, and the
  output would read like a port of Rust.
- **`fmt::Result` as a real `Result`.** Every `write!(f, ..)?` would test
  for an `Err` that can't happen.

## Consequences

- A generic function with a `T: Display` bound takes a `TDisplay` argument.
- A JS caller can call a `Display` impl's function directly for the string:
  `pointDisplay_fmt(p)`.
