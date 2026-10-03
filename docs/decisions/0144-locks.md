# 0144. A lock on one thread is a `RefCell`, and an `Arc` is an `Rc`

Status: Accepted. Extends [0023](0023-strings-references-shared-state.md)
and [0025](0025-vec-loops-refcell-mut.md).

## Context

Shared state in Rust that may cross threads is `Arc<Mutex<T>>` or
`Arc<RwLock<T>>`, and it's how much Rust is written even where it never
does:

```rust
let stats = Arc::new(Mutex::new(Stats { hits: 0 }));
let other = Arc::clone(&stats);
other.lock().unwrap().hits += 1;
```

Both were errors: `Arc` wasn't an `Rc`, and `lock()` wasn't anything.

JS runs a module on one thread, and `std::thread::spawn` is an error
(ADR 0142), so no other thread can hold a lock: a lock is a `RefCell` that
nothing contests.

## Decision

**A `Mutex` or an `RwLock` is `{ value }`, as a `RefCell` is, and an `Arc`
is the value it points at, as an `Rc` is:**

| Rust | JS |
|---|---|
| `Arc::new(x)`, `Arc::clone(&a)` | `x`, `a` |
| `Mutex::new(x)`, `RwLock::new(x)` | `{ value: x }` |
| `m.lock()`, `l.read()`, `l.write()` | `{ TAG: "Ok", _0: m.value }` |
| `m.lock().unwrap()`, `.expect(..)` | `m.value` |
| `*m.lock().unwrap() += 1` | `m.value = (m.value + 1) >>> 0` |
| `m.into_inner()`, `m.get_mut()` | `Ok` of `m.value` |

- **A lock is always `Ok`:** nothing else can hold it, and a guard isn't
  poisoned without another thread's panic. `unwrap()` of that `Ok` is its
  value, as for any `Ok(x)` just made.
- **A guard is what it guards,** `MutexGuard`, `RwLockReadGuard` and
  `RwLockWriteGuard` as `Ref` and `RefMut` are (ADR 0025): one held in a
  variable is the object.
- **A guard of a number, a string or the like held in a variable names the
  cell's `value`,** as a `&mut` held in one names its place (ADR 0099):
  `let mut n = m.lock().unwrap(); *n += 1;` is `m.value = (m.value + 1) | 0`,
  and so is a `RefCell`'s `borrow_mut()`. While the guard lives nothing
  else can use the cell, so a write through it is a write to `m.value`. Its
  place is fixed where it's locked: `slots[i].lock()` keeps the `i` of
  that moment. One of a cell that isn't a place, a call's result, is still
  an error.
- **An `Arc` is wherever an `Rc` is:** shown, compared, cloned, defaulted
  and written to JSON as what it holds.
- **Still errors:** `try_lock`, `is_poisoned`, `Arc::strong_count`, `{:?}`
  of a lock, which shows whether it's locked, and a lock of a value with a
  destructor.

## Why

- **It's exact on one thread.** A lock only changes what a program does
  when another thread holds it, and none can.
- **It's the JS a person writes:** a shared object, and a field read or
  written, with no lock at all.

## Alternatives

- **A lock flag,** so a second `lock()` while a guard is alive would
  deadlock or panic as Rust's does. Rust's own behavior there is a hang,
  which no program wants, and the flag would cost every access, as
  ADR 0025 found for `RefCell`'s borrow flag.

## Consequences

- A program that locks twice on one thread, `m.lock()` while a guard of
  `m` is alive, hangs natively and doesn't in JS: nothing checks the lock,
  as nothing checks a `RefCell`'s borrows.
- `drop(guard)` of a guard, which has no destructor in JS, is nothing.
