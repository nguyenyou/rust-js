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

**A value's drop is written where it's dropped**: the user's `drop`, then
each part that has one, in Rust's order: a struct's fields in declaration
order, a `Vec`'s or an array's items, an `Option`'s value, an enum's
variant's fields, a `Box`'s value.

```rust
struct Noisy(u8);
impl Drop for Noisy { fn drop(&mut self) { println!("drop {}", self.0); } }
struct Pair { a: Noisy, b: Noisy }
```

```js
noisyDrop_drop(pair.a);
noisyDrop_drop(pair.b);
for (const item of list) {
  noisyDrop_drop(item);
}
```

A type of the crate's own whose drop is longer than eight drops, or that's
inside itself, as a list is, gets a function of its own instead, which
calls itself for the ones inside: `const dropList = (list) => { .. }`,
declared before the drop that calls it, as `Clone`'s is (ADR 0052).

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
  const b = [2];
  let b$live = true;
  try {
    if (ready()) {
      b$live = false;
      consume(b);
    }
    console.log("end");
  } finally {
    if (b$live) {
      noisyDrop_drop(b);
    }
    noisyDrop_drop(a);
  }
}
```

- **A moved variable isn't dropped:** one that's moved anywhere gets a
  flag, `b$live`, cleared as it moves. Moved into a call, it moves as the
  call's made, after every operand, so an operand that panics first leaves
  it owned: `const arg = second(); again$live = false; pair(again, arg);`.
- **`let`s in a row share a `try`** when nothing between them can leave
  early: a `let` of a literal, a variable, or what's built of them.
- **A statement's value is dropped at once:** `Noisy(1);` and `let _ =
  Noisy(1);` are `noisyDrop_drop([1]);`.
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
    noisyDrop_drop(pair.b);
  }
  ```
- **A temporary is dropped at the end of its temporary scope**, as rustc's
  region scope tree gives it, so the edition's rule is rustc's. It's a
  `const` of its own, and the rest of its statement is a `try` whose
  `finally` drops it; what the statement declares is declared before the
  `try`, for what comes after. One a `let` keeps alive, `let r =
  &make();`, is owned by the rest of the block, as `r` would be.

  ```js
  const noisy = make("b");
  let first;
  try {
    first = noisy[0];
  } finally {
    noisyDrop_drop(noisy);
  }
  ```

  (Amended by ADR 0131: a statement that can't fail, as `first = noisy[0];`,
  has its temporaries' drops straight after it, with no `try`.)
- **A value made before an operand that may leave early is a temporary
  too,** `f(make(1), g())`: its flag clears as the call is made, after
  every operand, so a `g` that panics leaves it owned, and dropped.
- **An assignment drops the old value** after the new one is computed, as
  Rust does.
- **A generic function drops a `T` through a drop function it's given**,
  as JS hands generic code what depends on the type, the way `sort` takes a
  comparator: `dropT`, after its other arguments and dictionaries. Only a
  function that something in the crate gives, for `T`, a value with a
  destructor takes one, directly or through a generic function of its own
  that passes its own `dropU` on, so generic code no such value reaches is
  what a person would write, with no drop argument at all. A caller whose
  `T` has nothing to run passes none; one whose drop is its `drop` passes
  that, `noisyDrop_drop`, and another an arrow. (Amended as it was done:
  read as first written, nearly every generic function that owns a `T`
  would take one, since a panic before a move leaves it owned.)

  ```rust
  fn consume<T>(value: T) {}
  consume(Noisy(1));
  consume(5);
  ```

  ```js
  function consume(value, dropT) {
    dropT?.(value);
  }
  consume([1], noisyDrop_drop);
  consume(5);
  ```
- **A generic trait impl is given a drop for each type parameter that isn't
  `Copy`,** and so is each of its methods: they're called through the
  impl's dictionary, or resolved where they're called, by callers the walk
  for who gives what can't see, and what one drops needn't be in its own
  body. A helper it lends its value to may drop a clone, a std method, as
  `clear`, drops what it removes, and a default of the trait drops what
  it's given. Only a derive whose body drops nothing isn't: `Clone`'s,
  `Copy`'s, `Debug`'s, `Default`'s and the comparisons', and serde's,
  whose codecs rust-js writes. A method that drops nothing is given a drop
  it doesn't use, the price of not needing to prove it. The dictionary's
  accessor takes the drops after its dictionaries,
  `wrapConsume(TClone, dropT)`, is cached by both, weakly, and passes both
  on in each method: `consume: (arg0) => wrapConsume_consume(arg0, TClone,
  dropT)`. A default copied into it drops its trait's `Self` as the impl's
  type does: that drop is made from the impl's, `const dropSelf = (wrap) =>
  { dropT?.(wrap[0]); }`, and it's the body's drop for `Self`, which, as a
  type parameter of the trait, isn't the impl's `T`. A generic function the
  default gives its value to, `discard(self)`, is given a drop as a caller
  that has one gives it, so a default's `Self` counts as given one in the
  walk. One rust-js can't make, an `Rc<T>`'s, is an error only where the
  body drops a `Self`: a default that borrows it needs none. Found in review, across
  crates, but true within one: the impl's `T` had never been dropped, and
  then a default had dropped its `Self` with `T`'s drop. (Amended as it was done: decided first from
  the signatures, then from each method's body, each of which missed a
  way to drop one.)
- **A trait's own type parameters, and its `Self`, are the impl's,** so a
  call of a trait method given a value with a destructor for one is given
  its drop by the impl's dictionary, made where the impl is chosen:
  `sinkTakeA(noisyDrop_drop).take(sink, noisy)`, through a generic
  function, a default, a `dyn` or a `T::make(a)` with no `self`. A generic
  method's own type parameters aren't given one: those are an error where
  a type may have a destructor, of the crate's own trait where it's
  declared, and of a library's where it's called. (Amended: all of a
  trait method's type parameters but `Self` were an error.)
- **A crate with no destructor, and no library's, gives no drops**: no
  value in it has one to run, so its generic impls take no `dropT`, and
  a dictionary that takes nothing else is one object. A library still
  does, as its consumers' values may have one. (Amended: every generic
  impl took one.)
- **A closure that takes a value with a destructor by value holds it:**
  the variable it took is moved into it, where it's made, and the
  closure's drop drops what it holds, in the order it took them, through
  the variables themselves, which a JS arrow shares. A closure called once,
  whose body moves what it holds, owns it in its body, as a function owns
  its parameters: what it doesn't move is dropped as the call ends, after
  its parameters, in a `finally`. One that only reads or writes what it
  holds drops it where its own scope ends, or a generic function's `dropF`
  does:

  ```rust
  let tick = move || drop(a);
  call_once(move || x.len());
  ```

  ```js
  const tick = () => {
    let a$live = true;
    try {
      a$live = false;
      noisyDrop_drop(a);
    } finally {
      if (a$live) noisyDrop_drop(a);
    }
  };
  call_once(() => ..., (value) => { noisyDrop_drop(x); });
  ```

  Its drop sees what it took only where JS sees those variables: it's
  made for a `let` or a call, and an error made anywhere else, as a
  block's value, assigned, or dropped outside the body it's made in. One
  that isn't called once by its own body, `Fn` or `FnMut`, isn't given to
  a call by value, which may consume it without its body dropping what it
  holds. One that takes part of a value, `t.0`, is an error. rustc's own
  walk of a body doesn't reach a closure's captures: the facts walk them
  itself. (Amended: a closure that held one was an error.)
- **A loop that owns its items owns each for a time round**, as a function
  owns its parameter: its binding's, an unnamed one's for `_`, or what a
  pattern leaves of it, dropped as the round ends, by `continue` and
  `break` too, in a `finally` inside the loop. What it iterates, a `Vec`,
  an array, an `Option` or `iter::once(x)`, or their `into_iter()`, is
  walked as a JS iterator, `items`, and a `finally` around the loop drops
  what `items` hasn't reached, in order, as Rust's iterator does when it's
  dropped:

  ```js
  const items = v.values();
  try {
    for (const n of items) {
      try { .. } finally { noisyDrop_drop(n); }
    }
  } finally {
    for (const left of items) noisyDrop_drop(left);
  }
  ```

  A loop over an iterator of the crate's own owns each item the same way,
  and drops nothing else. A loop over an adapter of owned items, and over
  an iterator that holds a value with a destructor itself, is an error.
  rustc writes a loop with an `iter` variable and a `next()` its lowering
  doesn't: the facts see the loop as it's lowered. (Amended: a loop's item
  that owned one was an error.)
- **`mem::drop(x)` drops `x`, and `mem::forget` and `ManuallyDrop` don't.**
  A static is never dropped (ADR 0096). `mem::swap(&mut a, &mut b)` is `const t = a;
  a = b; b = t;` and `mem::replace(&mut a, v)` `const old = a; a = v;`, of
  a variable, a field or a box (ADR 0074): neither drops what it moves
  out, which is the other place's, or returned.
- **A std call rust-js doesn't know keeps what it takes is rejected** if
  what it takes may hold a value with a destructor, including one whose
  destructors rust-js can't follow, such as a `vec::IntoIter` of them:
  `skip_while` drops what it skips, where the JS wouldn't. Found once
  `mem::replace` let a rustc test reach it; the check had passed any value
  it couldn't follow.

**Rejected for now, with an error that says so:** an `Rc`, an `Arc` or a
thread-local holding a value with a destructor (it runs when the last
reference goes, which JS doesn't count), and a `dyn Trait` of one.

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
- **Found in implementing:** leaving a move's flag out when the move is a
  statement of the variable's own scope drops too little: a panic between
  the `let` and the move leaves it owned, and Rust drops it. So every moved
  variable has a flag. For the same reason, a variable moved into a call is
  moved after its other operands, and a value with a destructor made
  before an operand that can leave early, `f(Noisy(1), g())`, is an error
  for now: Rust drops it as `g` panics.
- **Found by rustc's tests:** a type whose parts double at each level,
  `S2<S2<T>>` in `S3<T>`, took exponential time to find what it drops,
  once for each path to a type, and didn't finish compiling; and its drop,
  written in place, would have been as long. What a type drops is found
  once for each type now, and a long drop is a function. The blessed run
  failed on it, as a new failure that crashed (ADR 0089).
- **Of rustc's tests, 39 more pass** (1,464 of 2,691), 31 of them ones that
  had stopped at a `Drop` impl. 40 stop at what destructors don't do yet,
  most often a borrowed temporary (13) and a value made before what may
  panic (7).
- **Found when shared references to a `static mut` let more of rustc's
  tests through (ADR 0096),** as each counts drops in one, three wrong
  answers: a parameter bound by `ref` or `_` wasn't dropped, though its
  function owns it however it's bound; a `let`'s value was taken as moved
  whatever its pattern, so `let _ = x` moved `x`, and `let ref r = f()`
  owned nothing; and a temporary dereferenced in place, a method call
  through a `Box` a call returned, wasn't taken for a temporary, and was
  never dropped. The safety net saw none of them: it knows only what's
  bound by value. Each is fixed, or an error, now.
- **With temporaries, generic code and partial moves,** 10 more of rustc's
  tests pass (1,482 of 2,691), and 28 stop at what destructors don't do
  yet, most often a temporary whose parts a pattern moves (6).
- **Done first, and not yet:** variables, parameters, moves, assignments,
  statements' values and `mem::drop`; then temporaries that end with their
  statement or a `let`'s block, and operands; generic code given a value
  with a destructor; and partial moves, by a field, by a `let`'s pattern
  and by a `match` arm's, whose bindings own what they bind, each a
  `const` of its own, for the rest of the block or the arm. A temporary of
  a condition or a block's tail, one made in a branch of its statement, one
  taken apart or partly moved, an `if let` that moves part of a value, a
  struct update from one, a `let x;` without its value, and `async` code
  that owns one are errors until they're done.
- A generic function given a value with a destructor has a JS parameter
  more than its Rust one has, as its dictionaries are (ADR 0052). A JS
  caller of an exported one passes none, and the drop doesn't run: a Rust
  value JS holds is never dropped either.
