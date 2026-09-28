# 0096. A `static` is its module's value, and a `static mut` its `{ value }`

Status: Accepted. Extends [0031](0031-consts.md) and [0037](0037-thread-locals.md).

## Context

`static` items are an error (ADR 0031, 0037). They're how Rust keeps one
value for the whole program: a table, a name, a counter. 156 of rustc's
tests stop at them, and what they use says what matters:

| What the tests use | Tests |
|---|---|
| immutable statics, and nothing below | 101 |
| `static mut` | 39 |
| a static holding a reference, `static S: &T` | 16 |
| atomics, `AtomicUsize` and the like | 9 |
| `extern` statics, thread-local statics, raw addresses | 2 each |
| `Mutex`, `RwLock`, `OnceLock`, `LazyLock`, threads | none |

A `static` differs from a `const` in two ways that JS can keep: it's one
place, so `&X` is the same every time, and a `static mut` can be written.
Its value, as a `const`'s, is worked out by rustc at compile time.

## Decision

**An immutable `static` is a `const` of its module, holding the value
rustc computed** for it, written as a `const`'s is (ADR 0031): numbers,
strings, arrays, objects. It's declared once, so `&X` is that one value
every time, and an object in it is one object.

```rust
static ORIGIN: Point = Point { x: 0, y: 0 };
pub static NAMES: [&str; 2] = ["a", "b"];
pub fn moved(dx: i32) -> Point { let mut p = ORIGIN; p.x += dx; p }
```

```js
const ORIGIN = { x: 0, y: 0 };
export const NAMES = ["a", "b"];
export function moved(dx) {
  let p = { ...ORIGIN };
  ...
}
```

- **A read of it is a read of a place:** a `Copy` value that the crate
  changes in place somewhere is copied as it's read (ADR 0020), as a
  `const`'s is.
- **It goes at the top of its module with the `const` items,** is exported
  when it's `pub` or another module uses it, and one inside a function goes
  there too, as a `const` does.

**A `static mut` is its module's `{ value }`,** as a thread-local's `Cell`
is (ADR 0037), holding the value rustc computed for it:

```rust
static mut COUNT: u32 = 0;
pub fn bump() -> u32 { unsafe { COUNT += 1; COUNT } }
```

```js
const COUNT = { value: 0 };
export function bump() {
  COUNT.value = COUNT.value + 1 >>> 0;
  return COUNT.value;
}
```

- **Reads and writes are `COUNT.value`,** from any module: an imported JS
  binding can't be assigned, and an object's field can.
- **A shared reference to one, `&COUNT`, is the value it points to**, as
  any shared reference is (ADR 0023): a number's is its value then, and an
  object's is the object. Rust makes writing the static while such a
  reference is still used undefined behavior, so no program Rust defines
  can tell. `println!("{}", COUNT)` takes one, as does a method of
  `&self`. (Amended: the first version rejected every reference; 22 of
  rustc's tests stopped there, most of them counting drops; 8 more pass,
  and most of the rest stop at a destructor, ADR 0098.)
- **`&mut COUNT` and `&raw mut COUNT` are rejected** for now: a `&mut` of a
  number has no JS value yet.

**An atomic is a `{ value }` too, as a `Cell` is**, as `static N:
AtomicUsize = AtomicUsize::new(0)` is `const N = { value: 0 }`. JS runs a
module on one thread, so every ordering holds, and each operation is the
plain one:

| Rust | JS |
|---|---|
| `N.load(o)`, `N.store(v, o)` | `N.value`, `N.value = v` |
| `N.swap(v, o)` | the old value, and `N.value = v` |
| `N.fetch_add(v, o)`, `fetch_sub`, `fetch_and`, `fetch_or`, `fetch_xor`, `fetch_max`, `fetch_min` | the old value, and `N.value` changed as the integer type wraps (ADR 0011) |
| `N.compare_exchange(a, b, o1, o2)` and `_weak` | `Ok(old)` and `N.value = b` if it was `a`, else `Err(old)` |

A 64-bit atomic's value is a BigInt (ADR 0086). The orderings, and what
their arguments do, are evaluated, as any argument is.

**Rejected, with an error that says so:** an `extern` static (a foreign
symbol), a `#[thread_local]` static (unstable), a raw address of any
static (`&raw const X`, `addr_of!`), and a static whose value has no value
tree and isn't an atomic's, as one holding a reference to another static,
a function pointer, or a `dyn`.

## Why

- **It's what Rust means:** one place, whose value is fixed at compile time
  and, for a `static mut`, written where rustc's `unsafe` allows it.
- **Nothing depends on load order:** the values are rustc's, so statics,
  as constants, don't read each other as modules load (ADR 0031).
- **The JS is what a JS programmer would write for state:** a module's
  `const`, and a `{ value }` where it changes, which a JS caller can read
  and write too.

## Alternatives

- **Lower the initializer, as thread-locals are:** it would show how the
  value was written, but statics would have to be ordered, and couldn't be
  across modules that import each other (ADR 0031's reason).
- **A `static mut` as a module's `let`:** it reads better, `COUNT += 1`,
  but only its own module can assign it, and a `let` another module writes
  would need a setter.
- **Leave `static mut` out:** Rust discourages it, but 39 of the tests use
  it, and so does code written before thread-locals and atomics.

## Consequences

- A `static` is made when its module loads, from a literal, so there's
  nothing for it to run.
- `Sync` means nothing on one thread: a static rustc accepts is one JS
  value, whatever its type's `Sync` says.
- A `pub static mut` is an exported `{ value }` that JS can change, as
  Rust code can, in `unsafe`.
- Left for later: a `&mut` to a `static mut`, a static holding a
  reference to another, and `Mutex` or `OnceLock` in a static, which none of
  the tests use.
- **rustc makes value trees only of constants,** so a static's is read out
  of the memory rustc computed for it, a field at a time with rustc's own
  destructuring of a constant, following each `&` in it. That gives a
  struct a variant index too, which only an enum's value tree has: the
  first version read `Point { x: 3, y: 4 }` as `{ x: 0, y: 3 }`, and
  `statics.rs` found it.
- An atomic is `{ value }` wherever it is: `new`, `default()`,
  `into_inner` and `{:?}` are a `Cell`'s. `fetch_nand`, `fetch_update`,
  `get_mut` and `AtomicPtr` are rejected for now.
- **Of rustc's tests, 76 more pass** (1,411 of 2,691), each of which had
  stopped at a static; 29 still stop at what's rejected here, most often a
  function pointer in a static (9) or a reference to a `static mut` (7).
- A `thread_local!`'s own storage is a static std writes inside it, which
  JS needs none of (ADR 0037): it isn't one of the crate's statics.
- `#[thread_local]` is unstable, so the corpus, whose cases are modules,
  can't enable it: its rejection is a diagnostics test.
