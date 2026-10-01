# 0131. A temporary taken apart owns what its pattern leaves

Status: Accepted. Extends [0098](0098-destructors.md).

## Context

A value with a destructor that a pattern takes apart where it's made was an
error: `let (x, _) = (make(3), make(4));`, `match (make(5), make(6)) { (p,
_) => .. }`, `match pick(n) { Two::Both(a, _) => .. }`, and `if let (Some(z),
_) = (Some(make(7)), make(8))`. A variable taken apart was supported
already (ADR 0098): what the pattern binds is the bindings', and the rest
is the variable's, each part with a flag where it moves on only some paths.

Rust drops what a pattern moves out of a temporary as its binding's scope
ends, and the rest as the temporary's scope ends: the end of its `let` or
`match` statement, or, in Rust 2024, of an `if let`'s `if`.

## Decision

**A temporary taken apart is dropped as a variable taken apart is: what
the pattern moves out is its bindings', and the temporary's scope drops the
rest.**

```rust
let (x, _) = (make(3), make(4));
```

```js
temporary = [loud, arg];
x = temporary[0];
loudDrop_drop(temporary[1]);
```

```rust
match (make(5), make(6)) { (p, _) => .. }
```

```js
temporary = [loud, arg];
temporary$0$live = true;
try {
  temporary$0$live = false;
  p = temporary[0];
  ..
} finally {
  if (temporary$0$live) {
    loudDrop_drop(temporary[0]);
  }
  loudDrop_drop(temporary[1]);
}
```

- **A part a `let` moves is moved on every path**, so it has no flag; a
  part an arm or an `if let` moves has one, cleared as it binds.
- **A temporary a `ref` binding keeps for the block**, `let (a, ref b) =
  (make(40), make(41));`, is the block's as `let r = &make();`'s is (ADR
  0098), less what the `let` moves out: `a`'s, dropped before it, as Rust
  drops it.
- **A `match` of a tuple taken apart builds the tuple**, the temporary
  that owns what's left, rather than testing each part where it is.
- **An `if let` that's a whole statement, with no `else`, drops its
  scrutinee's temporaries as the statement ends**: the same moment as the
  `if`'s end, where Rust 2024 drops them. An `if let`'s bindings are its
  `then`'s, dropped as that ends, as an arm's are.
- **A variable an `if let` takes apart** moves the parts it binds, as a
  `match` arm does.
- **A parameter taken apart**, `fn f((a, ref b): (Loud, Loud))`, is the
  function's, less what its pattern moves out: `b`'s part is dropped as
  the function ends, after `a`, as Rust drops it, where it had been
  dropped never, a wrong answer. One that moves every part, `(a, b)`, was an
  error, and is the function's the same way.
- **A temporary's drops come straight after a statement that can't fail**,
  as `x = temporary[0];`, with no `try`, here and for any temporary, as in
  ADR 0098's example, whose `try` this removes.
- **Still errors:** a let chain that binds a value with a destructor,
  which a later test failing would have to drop before the `else`; an
  `if let` with an `else`, or inside an expression, taking a temporary
  apart; and a value with a destructor made before what may panic, in an
  arm's value rather than a statement.

## Why

- **It's exact**: what's dropped, and when, is what Rust drops then,
  compared with native Rust by the `temporaries_taken_apart` corpus case.
- **It's the JS a person writes**: a flag only where a part moves on some
  paths, and no `try` around what can't fail.

## Consequences

- Temporaries taken apart by `let`, `match` and an `if let` statement
  compile.
- `drop_temporaries`' JS has one `try` fewer.
