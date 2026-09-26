# 0066. Text with values in it is a template literal

Status: Accepted. Changes [0034](0034-strings-and-chars.md); extends [0065](0065-format-with-oxfmt.md).

## Context

`format!`, `write!`, `{:?}` and `panic!` build their string from text and
values joined with `+` (ADR 0034):

```js
name + ": " + String(n) + " item" + (n === 1 ? "" : "s")
```

It's correct, but it isn't what a person writes today, and once formatted
(ADR 0065), a long one is laid out one piece per line.

## Decision

**Strings joined of text and values are a template literal:**

```js
`${name}: ${n} item${n === 1 ? "" : "s"}`
```

- **All in one place.** Every string these build is joined by one function,
  `join`, which decides:

  | Pieces | JS |
  |---|---|
  | text only | a string: `"a dot"` |
  | text and values | a template: `` `Some(${x})` `` |
  | values only | `a + b`, or the value alone |

- **`String(x)` is `x` in `${}`**: a template makes each value a string as
  `String` does, so `${n}` is `String(n)`.
- **Joins inside a join are taken apart**, so a template doesn't nest one
  that it could hold: `` `Some((${a}, ${b}))` ``. A value that's a choice
  keeps its own: `` `${x == null ? "None" : `Some(${x})`}` ``.
- **The text is escaped as a template needs:** `` ` ``, `\` and `${`,
  and controls as in a string (`\n` stays `\n`, not a line break).
- `{:?}` of a constant `Option` is known: `Some(1.0)` is `"Some(1.0)"`, not a
  test of `1 == null`.

## Why

- **It's how JS is written now**, and it reads as the Rust template does:
  `"{name}: {n} item"` and `` `${name}: ${n} item` `` line up.
- **Formatters leave a template on one line**, as a person would, rather
  than a chain of `+` broken one piece per line.

## Alternatives

- **`+` everywhere** (as before): no nesting, but noisy, with a `String(..)`
  around each number.
- **A template even for values alone** (`` `${a}${b}` ``): uniform, but
  `a + b` of two strings is plainer.
