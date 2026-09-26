# 0037. `thread_local!` is a variable of its module

Status: Accepted.

## Context

A web program keeps state between events: how many times something ran,
the element currently on screen, a log. The playground keeps three such
values for its Result frame. In JS they're variables at a module's top level.

Rust keeps such state in a `static`. A `static` must be `Sync`, and JS values
aren't: an `&'static Element` can't go in a `Mutex`. So Rust programs on
the web keep JS values in `thread_local!`, as wasm-bindgen's documentation
recommends:

```rust
thread_local! {
    static COUNT: Cell<u32> = Cell::new(0);
}
```

`static` items were an error in rust-js (ADR 0016). ReScript has top-level
`let`s holding `ref`s, and Scala.js has `object`s with `var`s.

## Decision

**A thread-local is a `const` of its module, holding its value**, made from
the macro's initializer:

```rust
thread_local! {
    static COUNT: Cell<u32> = Cell::new(0);
    static LOG: RefCell<Vec<String>> = RefCell::new(Vec::new());
}
pub fn bump() -> u32 { COUNT.set(COUNT.get() + 1); COUNT.get() }
```

```js
const COUNT = { value: 0 };
const LOG = { value: [] };
export function bump() {
  COUNT.value = COUNT.value + 1 >>> 0;
  return COUNT.value;
}
```

- **JS runs a module on one thread**, so "one per thread" is "one", the
  module's own variable. It goes at the top of the file with the `const`
  items (ADR 0031), and is exported if another module uses it.
- **`thread_local!` expands to `const NAME: LocalKey<T>`**, whose block holds
  `fn __rust_std_internal_init_fn() -> T { init }` and std's storage for it,
  a `static` of each target's kind. rust-js lowers the `init` function, uses
  its body as the value, and leaves the storage out.
- **The methods**, on the `Cell` or `RefCell` it holds (ADR 0023):

  | Rust | JS |
  |---|---|
  | `KEY.get()`, `KEY.set(v)` | `KEY.value`, `KEY.value = v` |
  | `KEY.with(f)` | `f(KEY)` |
  | `KEY.with_borrow(f)`, `KEY.with_borrow_mut(f)` | `f(KEY.value)` |

## Why

- **It's how Rust programs for the web already keep state**, and rustc
  checks every use of it.
- **The JS is the variable a JS programmer would write**, with nothing of
  std's storage left.

## Alternatives

- **`static mut`**: plain, but `unsafe` at every use, and Rust 2024 warns
  about references to one.
- **`static` of a `Sync` type** (atomics, `Mutex`, `OnceLock`): right for
  numbers, but JS values aren't `Send` or `Sync`, so it can't hold an
  element.
- **Pass the state around**: works, but a web page's event handlers all need
  the same state, and that's what a module's variables are for.

## Consequences

- **A thread-local is made when its module loads**, not at first use as in
  Rust. An initializer with side effects, or one reading another module's
  thread-locals, runs at load. Initializers like `Cell::new(0)` and
  `RefCell::new(Vec::new())` can't tell.
- `with` and `with_borrow_mut` call their closure right there:
  `((log) => log.push(line))(LOG.value)`.
- Plain `static`s are still an error, and so are `take` and `replace` on a
  thread-local.
