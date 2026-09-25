# 0008. Two lowering modes: expressions and statements

Status: Accepted

## Context

This is the central problem of compiling Rust to JS.

In Rust, *everything* is an expression. `if`, `match`, `loop` and blocks all
produce values:

```rust
let x = match n { 0 => 1, _ => { let t = n * 2; t + 1 } };
```

JS separates statements (`if`, `while`, `let`) from expressions (`a + b`,
`f(x)`, `c ? a : b`). You can't put a `while` inside a `+`.

## Decision

Lower each THIR expression in one of two modes:

```
expr(e)  ──► a JS expression          "give me something I can put inside a + b"
               (may first push statements it needs, like a block's `let`s)

stmt(e, dest) ──► JS statements        "do this, then put the value HERE"
                    dest = Return          → return v;
                         | Assign(name)    → name = v;
                         | Discard         → v;   (or nothing, if v has no effects)
```

They call each other:

- `stmt` handles control flow (`if`, `match`, loops, `return`, `break`,
  assignment) directly, passing `dest` down into each branch. Anything else
  it hands to `expr`, then delivers the result to `dest`.
- `expr` handles "simple" expressions directly (literals, variables,
  operators, calls, and `if` with simple branches as a ternary). For control
  flow it declares a temporary and calls `stmt(e, Assign(tmp))`.

A function body is `stmt(body, Return)`, or `stmt(body, Discard)` if the
function returns `()`.

## Why

**Passing `dest` down is what makes the output read naturally.** Look at
`fib`:

```rust
if n < 2 { n } else { fib(n - 1) + fib(n - 2) }   // with dest = Return
```

`Return` flows into both branches, so each branch returns directly:

```js
if (n < 2) {
  return n;
} else {
  return fib(n - 1 >>> 0) + fib(n - 2 >>> 0) >>> 0;
}
```

No temporary, and no `let result; ... return result;`. The same trick turns
`break value` into `return value` when the loop's value is being returned
(see [0015](0015-loops.md)).

## The unit/never rule

At the top of `stmt`: if the expression's type is `()` or `!`, its
destination becomes `Discard`.

- `()` carries no information, so `x = ()` is pointless.
- `!` (never) means the value never arrives. `return return 5` is silly;
  just lower the inner `return`.

This one rule removes a whole class of odd output.

## Alternatives

- **Everything as an expression**: wrap statements in IIFEs,
  `(() => { ... })()`. Always works, and always unreadable and slow.
- **Everything as statements**: every subexpression gets a temporary. Correct,
  but reads like assembly.

## Consequences

- `if` in statement position is always an `if` statement. In expression
  position it's a ternary only when both branches are simple, otherwise a
  temporary. See [0009](0009-temporaries-and-evaluation-order.md) for what
  "simple" means.
