# 0146. A generic associated type of lifetimes is an associated type

Status: Accepted. Extends [0106](0106-generic-traits.md).

## Context

A generic associated type, `type Iter<'a>`, was an error: an associated
type was supported only without parameters of its own (ADR 0106).

```rust
trait Container {
    type Item<'a>: Debug where Self: 'a;
    type Iter<'a>: Iterator<Item = Self::Item<'a>> where Self: 'a;
    fn items<'a>(&'a self) -> Self::Iter<'a>;
}
```

Its parameters are almost always lifetimes, so a borrowing iterator can
name what it borrows. JS has no lifetimes: every `Iter<'a>` is one type to
it. What an associated type needs from rust-js, a dictionary entry for
each bound it declares (ADR 0106), is the same for each.

## Decision

**A generic associated type whose own parameters are lifetimes is an
associated type: its lifetimes are erased, and each bound it declares is
one entry in its trait's dictionary, as a plain one's is.**

```js
function show(c, CContainer) {
  const arg = CContainer.last(c);
  return arg == null ? "None" : `Some(${CContainer.ItemDebug().fmt($someValue(arg))})`;
}
```

- **One with a type or const parameter and no bound** is the type it is
  where it's used, `type Array<const N: usize> = [u8; N]`, and unknown in
  generic code, as any associated type is: no dictionary entry would vary
  by its parameter.
- **One with a type or const parameter and a bound** is still an error,
  `type Of<T>: Clone`: its entry would be a function of `T`'s evidence.
- **An unknown associated type that's an iterator** may be an array or a
  lazy JS iterator, as a type parameter may: generic code takes it with
  `Iterator.from` (ADR 0061). (Amended: it was taken to be an array, so
  `s.items().count()` of a `Successors` was `undefined`.)
- **`.next()` of one taken where it's made,** `c.items().next()`, steps
  `Iterator.from` of it, the iterator itself, not the box a `&mut` to a
  generic value is (ADR 0099).

## Why

- **It's exact:** a lifetime changes nothing JS can see, and the
  `generic_associated_types` corpus case compares borrowing, copying and
  lazy iterators, a bound used in generic code, and a const parameter,
  with native Rust.
- **It's the JS a person writes:** the same dictionary as without the
  lifetime.

## Consequences

- 6 rustc tests that stopped at a generic associated type compile; the 2
  others stop at what they do with one, a `&mut` to an iterator of a type
  parameter and `get_mut` of a range.
- `.next()` of an unknown iterator kept in a variable is still an error,
  as of a type parameter's: `Iterator.from` of an array starts again.
