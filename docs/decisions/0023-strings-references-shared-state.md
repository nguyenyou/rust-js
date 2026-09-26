# 0023. Strings, references and shared state

Status: Accepted. Extended by [0034](0034-strings-and-chars.md): more string
methods, `char`, and `format!`.

## Context

An imperative web program needs three more things:

- **Text**: `&str`, `String`, and numbers turned into text.
- **References**: `&Element` handles, `&str` arguments.
- **State shared by event handlers.** Every handler must own what it
  captures, so two buttons changing one count is `Rc<Cell<i32>>` in Rust:
  `Rc` to share it, `Cell` to change it through a shared reference.

std's `String`, `Rc` and `Cell` are built on raw pointers and `unsafe`, so
we can't compile their source. We don't need to: each has an obvious JS
meaning.

## Decision

| Rust | JS |
|---|---|
| `&str`, `String`, `"hi"` | a JS string, `"hi"` |
| `&T` | the `T` itself |
| `*r` | `r` |
| `Box<T>`, `Box::new(x)` | `T`, `x` |
| `Box<dyn Fn..>` | the JS function (ADR 0022) |
| `Rc<T>`, `Rc::new(x)`, `rc.clone()` | `T`, `x`, `rc`: the same object |
| `Cell<T>`, `Cell::new(x)` | `{ value: x }` |
| `c.get()`, `c.set(v)` | `c.value`, `c.value = v` |
| `n.to_string()` (integers, `bool`) | `String(n)` |
| `s + &t` (`String`) | `s + t` |
| `s.to_string()` (`&str`, `String`) | `s` |
| `Deref` of `String` or `Rc` | the value itself |

rust-js recognizes these std items by their rustc diagnostic items and lang
items (`box_new`, `to_string_method`, `deref_method`, `Rc`, `Cell`,
`CloneFn`, `Add`), not by name. Anything else from std is reported as not
supported, with its path.

## Why

- **A shared reference can't change anything**, so it doesn't matter
  whether JS copies or shares what it points to: it's just the value.
- **`Rc` is what a JS reference already is.** The garbage collector does
  the counting, so `Rc::clone` shares the object, as it does in Rust.
- **`Cell` needs a slot everyone can see**, and `{ value }` is the smallest.
  Since `Rc` shares the object, every handler holding the `Rc` sees each
  `set`.
- **JS strings are immutable**, and so is everything we support on them,
  so `String` and `&str` can both be the JS string. `String(n)` prints
  integers and `bool` exactly as Rust does.

## Alternatives

- **Strings as UTF-8 byte arrays**, matching Rust's representation:
  faithful to byte indexing, but every DOM call would need a conversion.
  Rust code that indexes bytes is rare in UI code and can be addressed
  when needed.
- **`Rc` as `{ value }` too**: faithful to `Rc::ptr_eq` and friends, but
  one more object and `.value` for nothing.
- **Compile std**: it's `unsafe` pointer code all the way down.

## Consequences

- `f64::to_string` isn't supported: JS writes `1e21` where Rust writes
  `1000000000000000000000`, and `Infinity` for `inf`.
- `&mut` references aren't supported yet, except through closure captures
  (ADR 0022). A `&mut` to a struct could be the object itself, but a `&mut`
  to a local number needs a place to point at.
- Growing a `String` (`push_str`), `RefCell`, `Vec` and the rest of std come
  next, the same way: one known item at a time, each with its JS meaning.
