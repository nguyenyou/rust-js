# 0007. A JS AST with a precedence-aware printer

Status: **Partly superseded by [0018](0018-print-with-oxc.md).** The AST half
stands: `lower.rs` still builds our small JS AST and never thinks about
parentheses. The printer half is replaced: oxc now prints, and `js.rs` no
longer contains a printer. The reasoning below is kept for history.

## Context

The lowering could print JS strings directly. But then every piece of code
that builds an expression has to decide where parentheses go, and it's easy
to end up with `((((a - b)) | 0))` everywhere or, worse, a missing pair that
changes meaning.

## Decision

Split code generation in two:

- `lower.rs` builds a **small JS AST** (`js::Expr`, `js::Stmt`) and never
  thinks about parentheses.
- `js.rs` **prints** it. The printer knows JS operator precedence and adds
  parentheses only where JS needs them.

The rule, for a binary operator with precedence *p*:

```
left operand:  parenthesize if its precedence <  p
right operand: parenthesize if its precedence <= p   (left-associative)
```

So `a - b | 0` prints as-is (`-` binds tighter than `|`), while `a - (b - c)`
keeps its parentheses.

## Why

- **One job each.** The lowering decides *meaning*; the printer decides
  *spelling*. A precedence bug can then live in only one place.
- **Readable by default.** Parentheses appear only where they change meaning,
  which is how a person writes JS.
- **The AST stays tiny**: only what we emit today (numbers, variables, unary,
  binary, ternary, calls; `const`/`let`, assignment, `if`, `while`, `break`,
  `continue`, `return`).

## Details worth knowing

- **Negative literals** count as unary-precedence expressions, so
  `x - -5` prints correctly.
- **`- -x`**: if a unary operator is followed by an operand starting with the
  same character, the printer adds a space. `--x` would be the decrement
  operator.
- **`else if`**: an `else` block containing only an `if` prints as
  `} else if (..) {`, which keeps `match` chains flat. See [0014](0014-match-lowering.md).
- **Formatting** is fixed: two-space indent, one statement per line, a blank
  line between functions.
- **Strings** are printed with Rust's `{:?}` escaping. That's correct for
  what we emit today (variant names). Arbitrary string literals will need a
  JS-specific escaper.

## Alternatives

- **Print strings directly**: fewer types, but parentheses logic spreads
  everywhere.
- **An off-the-shelf JS AST crate** (for example `swc`): complete but heavy,
  for the few node kinds we need.

## Consequences

- New JS constructs need a variant in the AST plus a printing rule. Small
  cost, clear place.
