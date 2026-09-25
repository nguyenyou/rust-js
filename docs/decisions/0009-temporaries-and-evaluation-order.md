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
