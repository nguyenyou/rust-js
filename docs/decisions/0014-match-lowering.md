# 0014. `match` becomes an `if` / `else if` chain

Status: Accepted

## Context

`match` has no direct JS equivalent. JS `switch` compares with `===` only,
and has no guards or ranges.

## Decision

Lower `match` to an `if` / `else if` / `else` chain:

```rust
match n { 0 => 0, 1 => 1, _ => fib(n - 1) + fib(n - 2) }
```
```js
if (n === 0) {
  return 0;
} else if (n === 1) {
  return 1;
} else {
  return fib_match(n - 1 >>> 0) + fib_match(n - 2 >>> 0) >>> 0;
}
```

The rules:

1. **Evaluate the scrutinee once.** If it's a variable, test it directly.
   Otherwise store it: `const match = <scrutinee>;`.
2. **Each arm's pattern becomes a boolean test** (`pattern_test`):

   | Pattern | Test |
   |---|---|
   | `_`, `x` | none (always matches) |
   | `0`, `true` | `s === 0`, `s === true` |
   | `Order::Ascending` | `s === "Ascending"` |
   | `A \| B` | `testA \|\| testB` |
   | `pat if cond` | `test && cond` |
3. **The last arm has no test**, if it has no guard. Rust already proved the
   match exhaustive, so if every earlier arm failed, the last one must
   match. It becomes a plain `else`. Arms after an always-matching arm are
   unreachable and are dropped.
4. **Bindings** (`x => ...`): if the subject can't change during the arm (an
   immutable variable, or our `const match`) and the binding isn't `mut`,
   `x` simply *reuses the subject's name*. Nothing is emitted. Otherwise it
   gets a real copy at the start of the arm: `const x = s;`.
5. The arm bodies get the match's own `dest` ([0008](0008-expression-and-statement-modes.md)),
   so each arm can `return` directly.

## Why `if` chains rather than `switch`

- **Uniform.** Guards, `|` patterns and (later) ranges are just boolean
  tests. A `switch` handles only the simplest case.
- **`break` keeps its meaning.** Inside a JS `switch`, `break` exits the
  *switch*. In Rust, `break` inside a match arm exits the enclosing *loop*.
  With `if` chains a Rust `break` is a JS `break`, with no labels needed.
- **It reads like the source**: one branch per arm, in the same order.

## Alternatives

- **`switch`** for integer/enum matches: marginally faster on huge matches,
  but see above.
- **Decision trees** (rustc's own match compilation): optimal, but the
  output no longer looks like the source.

## Consequences

- Not supported yet: patterns with fields, ranges (`1..=5`), `ref`
  bindings, bindings inside `|` patterns, `x @ pat`, and guards that need
  statements. A copied binding in a guarded arm is also rejected. Each gives
  a clear compile error.
- An arm's test is evaluated only if every earlier arm failed, and a body
  runs only after its test passes. So a body mutating the scrutinee variable
  can't affect any test. That's why testing a mutable variable directly is
  safe. Only *aliasing* a binding to it isn't (rule 4).
