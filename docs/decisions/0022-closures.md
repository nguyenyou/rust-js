# 0022. Closures are arrow functions

Status: Accepted

## Context

Web code is callbacks: every event handler is a closure. A Rust closure
captures variables either **by reference** (`&x`, `&mut x`) or **by value**
(a copy or a move: always with `move`, or when the body consumes the value).
Since Rust 2021 it may capture just part of a variable (`p.x`).

A JS arrow function captures *variables*, not values: it sees later changes
to them, and its own changes are seen outside.

## Decision

A closure is an arrow function, written where the closure is created:

```rust
let mut add = |x: i32| total += x;          let add = (x) => {
                                              total = total + x | 0;
                                            };
Box::new(move |x| x + k)                    (x) => x + k | 0
```

Its parameters follow the rules for function parameters, patterns included.
Calling it, `f(a, b)` (in THIR, `Fn::call(&f, (a, b))`), is `f(a, b)`.

Captures:

- **By reference**: the arrow uses the variable itself. That's what a Rust
  reference capture means, and the borrow checker has already made sure
  nothing else uses the variable while the closure can.
- **By value, from an immutable variable**: also the variable itself. It
  never changes, and the closure can't change it either.
- **By value, from a mutable variable**: the closure gets a *snapshot*
  first, and the arrow uses that:

  ```js
  let count$1 = count;   // move || { count += 1; count }
  let next = () => {
    count$1 = count$1 + 1 | 0;
    return count$1;
  };
  ```

  When only part of a variable is captured (`p.x`), the snapshot is of that
  part. It goes through the same read as any other (ADR 0020), so a `Copy`
  struct is copied if it needs to be.
- **No snapshot needed** when the capture is the variable's only use, and
  the closure isn't in a loop the variable was declared outside of. Nothing
  else can tell the difference then.

## Why

- **It's the JS a person would write.** An event handler reads as one.
- **Snapshots only where Rust and JS disagree.** Rust gives the closure its
  own copy; JS would share the variable. That's only visible if the
  variable can change, meaning it's mutable, and is used somewhere else.
- **The loop rule** matters because a closure made in a loop is made anew
  each time round, each with its own copy:

  ```rust
  let mut n = 0;
  while i < times {
      let mut bump = move || { n += 1; n };   // every bump starts from 0
      ..
  }
  ```

  Without a snapshot each time, every `bump` would share one `n`.

## Alternatives

- **Always snapshot by-value captures**: simpler, but puts `let x$1 = x;`
  in front of most closures, including every event handler that captures
  a handle.
- **Closures as objects with a `call` method**: models Rust's closure structs
  exactly, but no JS caller could use one as a callback.

## Consequences

- A closure can go to JS directly: `Box<dyn FnMut()>` is the arrow function
  itself (ADR 0023), so `add_event_listener(b, "click", Box::new(..))` passes it
  as a listener.
- Not supported yet: a by-value capture of a place other than a variable and
  its fields (through a reference), `async` closures, and coroutines.
