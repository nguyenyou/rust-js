# 0098. A destructor runs where rustc runs it, in a `finally`

Status: Accepted. Extends [0020](0020-structs-and-tuples.md) and [0052](0052-std-trait-impls.md).

## Context

A user `Drop` is an error (ADR 0097). It's the std trait rustc's tests
implement most: 128 of them, and for 110 the only one. With `Drop`
accepted and never called, 49 of those compile, so it's all that stands
in their way; they print or count in `drop` to check when it ran.

When a value is dropped is Rust's rule, and not a simple one:

- a variable at the end of its scope, in reverse order of declaration;
- a temporary at the end of its statement, or of the block, when a `let`
  extends it, which changed in Rust 2024 for a block's tail;
- nothing that's been moved, all of it or a field;
- the old value of a place assigned to;
- everything live, as a panic unwinds;
- a struct after its `drop`, field by field in declaration order; a
  `Vec`'s items in order; an enum's variant's fields.

JS has no destructors: a `FinalizationRegistry` runs when the collector
decides, if ever. rustc works these rules out as it builds MIR, which
rust-js doesn't lower. But what MIR building reads is in THIR already:
each variable's scope and each expression's temporary scope, from the
region scope tree, and each scope's end, `ExprKind::Scope`.

## Decision

**A type has a destructor to run if its drop reaches a user `Drop`**,
through its fields, variants, items or box. Only values of those types
get drop code; a crate without a `Drop` impl gets the JS it gets today.

**Each such type gets a drop function**, which calls the user's `drop`,
then drops each part that has one, in Rust's order:

```rust
struct Noisy(u8);
impl Drop for Noisy { fn drop(&mut self) { println!("drop {}", self.0); } }
struct Pair { a: Noisy, b: Noisy }
```

```js
function dropNoisy(noisy) {
  noisyDrop_drop(noisy);
}
function dropPair(pair) {
  dropNoisy(pair.a);
  dropNoisy(pair.b);
}
```

**A scope that holds one is a `try`, and its drops the `finally`**, where
rustc's scope tree ends it, in reverse order. A `finally` runs however the
scope ends: at its end, by `return`, `break` or `?`, and as a panic
unwinds.

```rust
fn main() {
    let a = Noisy(1);
    let b = Noisy(2);
    if ready() { consume(b); }
    println!("end");
}
```

```js
function main() {
  const a = [1];
  try {
    let b = [2], b$live = true;
    try {
      if (ready()) { b$live = false; consume(b); }
      console.log("end");
    } finally {
      if (b$live) dropNoisy(b);
    }
  } finally {
    dropNoisy(a);
  }
}
```

- **A moved variable isn't dropped.** One moved only by a statement of its
  own scope, not in a branch, a loop or a closure, is left out of the
  `finally`. One moved elsewhere gets a flag, `b$live`, cleared as it
  moves.
- **A field moved out isn't dropped, and the rest are, each on its own,**
  as a person cleaning up would: what's still owned. Rust forbids moving a
  field out of a type with a `Drop` of its own (E0509), so the value's
  drop is its remaining fields'. A field moved on some paths gets a flag,
  as a variable does.

  ```rust
  let pair = Pair { a: Noisy(1), b: Noisy(2) };
  consume(pair.a);
  println!("end");
  ```

  ```js
  const pair = { a: [1], b: [2] };
  try {
    consume(pair.a);
    console.log("end");
  } finally {
    dropNoisy(pair.b);
  }
  ```
- **A temporary is dropped at the end of its temporary scope**, as rustc's
  region scope tree gives it, so the edition's rule is rustc's.
- **An assignment drops the old value** after the new one is computed, as
  Rust does.
- **A generic function drops a `T` through a drop function it's given**,
  as JS hands generic code what depends on the type, the way `sort` takes a
  comparator. Only a function that drops a value of a type parameter takes
  one, after its other arguments and dictionaries, and a caller whose `T`
  has nothing to run passes none. A generic type's drop function takes its
  parameters' the same way: `dropWrapper(wrapper, dropT)`.

  ```rust
  fn consume<T>(value: T) {}
  consume(Noisy(1));
  consume(5);
  ```

  ```js
  function consume(value, dropT) {
    dropT?.(value);
  }
  consume([1], dropNoisy);
  consume(5);
  ```
- **`mem::drop(x)` drops `x`, and `mem::forget` and `ManuallyDrop` don't.**
  A static is never dropped (ADR 0096).

**Rejected for now, with an error that says so:** an `Rc`, an `Arc` or a
thread-local holding a value with a destructor (it runs when the last
reference goes, which JS doesn't count), a closure that captures one by
value, and a `dyn Trait` of one.

## Why

- **Rust's rules, from rustc:** the scopes are the ones MIR building uses,
  so what's dropped when is what native Rust does, edition by edition.
- **A `finally` is the one JS form that runs on every way out,** a panic
  included, which Rust's unwinding needs.
- **Nothing changes for code without a destructor,** which is almost all
  of it.

## Alternatives

- **Lower drops from MIR, where rustc elaborated them:** exact, flags
  included, but the rest of a function is lowered from THIR, and MIR's
  drops would have to be found again in it.
- **JS's `using` declarations:** they drop in reverse order, on a throw
  too, but a moved value would have to be taken out of its
  `DisposableStack`, and they're newer than any other JS rust-js writes.
- **A flag for every variable with a destructor:** simpler, and correct,
  but most are never moved and don't need one.

## Consequences

- A panic in a `drop` as another unwinds aborts in Rust; in JS its error
  replaces the first.
- A flag and a `try` are more JS than the program without drops had; it's
  what the Rust means.
- A generic function that drops a `T` has a JS parameter more than its
  Rust one has. Nothing depends on the old signatures yet, and rejecting
  generic drops would reject correct Rust for none of that.
