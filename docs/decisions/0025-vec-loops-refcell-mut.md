# 0025. `Vec`, `for` loops, `RefCell` and `&mut` to objects

Status: Accepted

## Context

A todo list is the smallest real web app: a list that changes. In Rust it's

```rust
struct State { todos: Vec<Todo>, next_id: u32, filter: Filter }
let state = Rc::new(RefCell::new(State { .. }));   // shared by every event handler

fn toggle(s: &mut State, id: u32) {
    for todo in &mut s.todos {
        if todo.id == id { todo.done = !todo.done; }
    }
}
```

That needs growable lists, `for` loops, a `RefCell` for state that event
handlers share and change, and `&mut` references.

## Decision

**`Vec<T>` is a JS array.** Its methods are recognized by rustc's
diagnostic items, like the rest of std (ADR 0023):

| Rust | JS |
|---|---|
| `Vec::new()`, `vec![a, b]` | `[]`, `[a, b]` |
| `v.push(x)` | `v.push(x)` |
| `v.len()`, `v.is_empty()` | `v.length`, `v.length === 0` |
| `v.clear()` | `v.length = 0` |
| `v.retain(keep)` | `$retain(v, keep)`: in place, a six-line helper |
| `v.iter()`, `v.iter_mut()` | `v` |
| arrays `[a, b]` | `[a, b]` |

**`for` over a sequence is `for…of`; over a range, a counting loop.**
rustc hands us `for` desugared into `into_iter`, `loop`, `next` and a
`match` on `Some`/`None`. rust-js recognizes that shape and puts the `for`
back:

```js
for (const todo of s.todos) { .. }        // for todo in &mut s.todos
for (let i = 0; i < n; i++) { .. }        // for i in 0..n
```

- **Sequences:** `&v`, `&mut v`, `v`, `v.iter()`, `v.iter_mut()`, arrays and
  slices.
- **Ranges:** the end is worked out once, as in Rust. If it could change
  (`0..n` with `n` changing inside the loop), it goes into a `const` first.
- **Loop variables:** a `mut` one gets its own copy, since changing it
  mustn't move the loop on. A pattern (`for (k, v) in pairs`) is taken apart
  at the top of each iteration. Labels, `break` and `continue` work as in
  ADR 0015.

**`RefCell<T>` is `{ value }`,** like `Cell` (ADR 0023). `borrow()` and
`borrow_mut()` are its `value`, so `*c.borrow_mut() += 1` is
`c.value = c.value + 1 | 0`, and `state.borrow_mut().todos.push(t)` is
`state.value.todos.push(t)`. A guard (`Ref`, `RefMut`) held in a variable is
the object it guards. That's only allowed when the guarded value is an
object: a guarded number in a variable would be a copy, not a place.

**`&mut` to an object is the object**, just as `&` is (ADR 0023). An object
here means a struct, tuple, `Vec`, `Cell`, `RefCell` or JS object. Changes
through the reference change the one object, which is what Rust means:

```js
function toggle(s, id) {                  // fn toggle(s: &mut State, id: u32)
  for (const todo of s.todos) { if (todo.id === id) todo.done = !todo.done; }
}
```

Two things are refused, because a JS variable can't point at another
variable. One is `&mut` to a number, a `bool` or a string. The other is
replacing a whole value through a `&mut` held in a variable (`*r = v`).

**Also:**

- `usize` and `isize` are 32 bits, as on `wasm32`.
- Strings get `trim()`, `is_empty()` (`length === 0`), `==` and `!=`
  (`===`, `!==`), and `String::new()`, `String::from(s)`, `s.to_owned()`
  and `s.as_str()`, the last three being `s` itself.
- `==` and `!=` on fieldless enums (a derived `PartialEq`) are `===` and
  `!==`, since their variants are strings (ADR 0013).

## Why

- **Arrays, `for…of` and objects are what JS already has.** The todo app
  compiles to JS a person would write, with one six-line helper.
- **Sharing is safe because Rust already checked it.** The borrow checker
  guarantees nothing else uses an object while a `&mut` to it lives, so
  handing JS the object itself can't be observed as aliasing.
- **Recognizing the `for` desugaring** gives readable loops. Compiling the
  iterator protocol literally would need `Option`, trait calls and a
  `while (true)` for every loop.

## Alternatives

- **JS iterators** (`v[Symbol.iterator]()`, `next()`): faithful to Rust's
  laziness, but slower, and nothing like hand-written JS.
- **A `Vec` class wrapping an array**: room for Rust's exact methods, but
  every JS caller would have to unwrap it.
- **Keeping `RefCell`'s borrow flag**, so that a second `borrow_mut()`
  panics as in Rust: faithful, but a counter on every access, in code the
  borrow checker already accepted. It could come back as a debug option.
- **`&mut` to numbers as `{ value }` boxes**: possible, but it changes the
  representation of every variable a reference is taken to.

## Consequences

- A program that would panic with "already borrowed" won't in JS: nothing
  checks `RefCell`'s rules at run time.
- `usize` arithmetic wraps at 2^32, not 2^64 as on a 64-bit native build.
  Tests that compare against native Rust only differ on overflow.
- `str::len` is refused: Rust counts UTF-8 bytes, JS counts UTF-16 units.
  JS's `trim` also removes U+FEFF, which Rust's doesn't.
- Not yet: indexing (`v[i]`), iterator adapters (`map`, `filter`,
  `enumerate`, `count`), `remove`, `insert`, sorting, `HashMap`, and `&mut`
  to numbers.
