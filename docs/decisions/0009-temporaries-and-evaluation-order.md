# 0009. Temporaries keep Rust's evaluation order

Status: Accepted

## Context

When `expr()` lowers something that needs statements (a block with `let`s, a
`match`), those statements are placed *before* the expression that uses
them. That's safe on its own, but consider:

```rust
f(a(), { let t = b(); t })
```

Naively hoisting the block gives:

```js
const t = b();     // b() now runs first...
f(a(), t);         // ...but Rust runs a() first!
```

If `a()` and `b()` have side effects, we've changed the program.

## Decision

Amended by [ADR 0084](0084-owned-phases-and-host-boundaries.md): operand and
conditional sequencing now use actual prerequisite statements, not `is_simple`
predictions. `is_simple` is a readability heuristic; points 1–2 below describe
the original implementation. The evaluation-order contract is unchanged.

1. **`is_simple(e)`** answers: "Can `e` become a JS expression with no
   statements before it?" Literals, variables, operators, calls with simple
   arguments, simple ternaries and blocks with no statements are simple.
   Control flow is not.
2. **`operands(list)`** lowers operands left to right. If some operand at
   position *k* is not simple, every earlier operand is saved in a
   `const tmp` first, unless it's a literal:

   ```js
   const tmp = a();
   const t = b();
   f(tmp, t);        // a() before b(), as in Rust
   ```
3. **`&&` / `||` with a complex right side** must not run the right side's
   statements unless needed:

   ```js
   let tmp = lhs;
   if (tmp) { /* rhs statements */ tmp = rhs; }   // `||` tests `!tmp`
   ```
4. **Compound assignment** `x += rhs` evaluates `rhs` first, then reads
   `x`, matching Rust's order for primitive types.

## Why

The rule is that **readable output must never cost correctness**. Temporaries
appear only in the rare cases where order could actually differ, so common
code (`fib(n - 1) + fib(n - 2)`) has none.

## Alternatives

- **Always spill every operand**: always correct, never readable.
- **Ignore the problem**: simpler, and wrong in exactly the cases that are
  hardest to debug.

## Consequences

- Variables are spilled too, not just calls, because a later block could
  assign to them: `f(x, { x = 5; x })`.
- Temporaries are named `tmp`, `tmp$1`, ... (see [0010](0010-naming-and-scopes.md)).
- **What's simple is what the JS has no statements for,** not what Rust
  looks like: `a || f(&mut y)` of a number `y` needs `y`'s box and its
  write-back around the call, so `f`'s call runs only if `a` doesn't
  decide, as a block's would; and a `while` whose condition has statements
  is `while (true) { ..; if (!c) break; .. }`, which runs them each time
  round, before the test. Both had run otherwise: the call always, and the
  condition read a variable before its declaration. Found by rustc's
  `lazy-and-or.rs` (`lazy_effects.rs`).
- Ternary candidates and `matches!` guards obey the same rule. Branch setup,
  calls and copy-back run only after the condition or pattern selects them.
  `if take { bump(&mut x) } else { 7 }` must leave `x` alone when `take` is
  false. The lowerer inspects both branches' actual `Evaluation`s and emits
  branch-local statements when needed (`conditional_regions.rs`).
