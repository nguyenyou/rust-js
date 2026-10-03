# 0137. `{:#?}` is pretty Debug, and a writer is told whether

Status: Accepted. Extends [0054](0054-display.md), [0060](0060-debug.md) and [0136](0136-common-std-methods.md). Its `alternate` parameter is superseded by [0143](0143-formatter-options.md): writers take an `options` object, `{ alternate: true }`.

## Context

`{:#?}` was an error everywhere: of a `Vec`, an `Option`, a derived `Debug`,
anything. It's what Rust programs print values with more than anything but
`{:?}`:

```
Point {
    x: 1,
    tags: [
        2,
        3,
    ],
}
```

Rust's rule composes: each part goes on a line of its own, indented by four
spaces, and ends with a comma; a part that's pretty itself has its own lines
indented too. A `Formatter` carries whether it's alternate into each `fmt`
it's given to, and `f.alternate()` asks. A `write!`'s own placeholders are
plain, whatever its `Formatter` is.

Re-formatting the plain string afterwards would be wrong: a hand-written
`fmt` that writes `[5]` with `write!` shows `[5]` under `{:#?}` too.

## Decision

**A crate that shows a value with `{:#?}`, or asks a `Formatter` if it's
alternate, gives each of its writers (ADR 0054) whether it's pretty, a
parameter after its value, `alternate`; and each part is shown as pretty as
what it's part of:**

```js
function pointDebug_fmt(point, alternate) {
  const shown = String(point.x);
  const shown$1 = $debugStr(point.label);
  return alternate
    ? $pretty("Point {", [`x: ${shown}`, `label: ${shown$1}`], "}")
    : `Point { x: ${shown}, label: ${shown$1} }`;
}
```

- **`$pretty(open, items, close)`** is the rule: each item on a line of its
  own, its lines indented by four spaces, ended by a comma; none, `[]`. A
  `finish_non_exhaustive()`'s `..` is a line of its own, with no comma.
- **Std's types are pretty as Rust's are**, where `{:#?}` asks, or as a
  writer's `alternate` says: `Some(..)`, tuples, `[..]`, `{..}` of a map or
  a set, `Ok(..)`, `RefCell { value: .. }`, and serde_json's `Array [..]`
  and `Object {..}`.
- **A derived `Debug`'s fields and a builder's arguments** (ADR 0136) are
  shown as pretty as their writer: the `&dyn Debug` they're given, which is
  the string it shows (ADR 0060), is made so. A `write!`'s placeholders
  are plain, as Rust's are.
- **A `&dyn Debug` made in a format's arguments for its `{:#?}`**, as
  `dbg!` makes one, is made pretty. `{:#?}` of one made anywhere else,
  which is a plain string already, is an error.
- **A writer that hands its `Formatter` on**, `self.0.fmt(f)` or a function
  of the crate's own, hands on its `alternate`; and a dictionary's `fmt`
  takes one, std's and the crate's.
- **`{:#}` of the crate's own `Display`** is given `alternate` too, for its
  `f.alternate()` to ask.
- **A crate with no `{:#?}`, and no `f.alternate()`, gets the JS it got**:
  its writers take no `alternate`. `{:#}` and `{:#x}`, alternate
  placeholders of no `Debug` argument, aren't a reason to.
- **A plain call is given no `alternate`** where nothing comes after it:
  `pointDebug_fmt(p)`.

## Why

- **It's exact**: the same lines, indents and commas as Rust's, from the
  same parts, compared with native Rust by the `pretty_debug` corpus case;
  and a `write!` stays as it's written.
- **It's the JS a person writes**: a flag passed down, and one helper for
  the rule, only where the crate asks for it.

## Consequences

- `{:#?}` and `f.alternate()` compile.
- A `Debug` builder kept in a variable is still an error (ADR 0136).
- The rustc suite compiles a test natively and as JS as at its own path,
  for what `file!()` and `dbg!` print to be the same.
