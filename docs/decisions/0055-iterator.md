# 0055. The crate's own `Iterator` is a JS iterator

Status: Accepted. Extends [0036](0036-iterators-and-sorting.md) and [0052](0052-std-trait-impls.md).

## Context

ADR 0036 made an iterator a JS array: `v.iter().map(f)` is `v.map(f)`. That
works because a slice's or a `Vec`'s iterator has all its items already.

A hand-written `impl Iterator` doesn't. Its `next` makes one item at a
time, and it may never stop:

```rust
impl Iterator for Fibonacci {
    type Item = u32;
    fn next(&mut self) -> Option<u32> { .. }
}
fibonacci().find(|&x| x > 50)
```

Turned into an array first, that `find` would never return. Rust's adapters
are lazy, and JS now has lazy ones too: its iterator helpers,
`Iterator.from(..).map(f).take(n)`.

## Decision

**The crate's own iterator is a JS iterator wherever it's used as one:**

```js
$iterator(fibonacci(), fibonacciIterator_next).find((x) => x > 50)
```

- **`next` is a function like any method of a trait impl:**
  `countdownIterator_next(c)`, returning an `Option`. A direct `c.next()`
  calls it.
- **`$iterator(it, next)` makes a JS iterator from it**, with
  `Iterator.from`. `None` ends it.
- **Lazy adapters stay lazy:**
  - `map`, `filter`, `enumerate`, `find`, `any`, `all`, `for_each`, `fold`
    and `sum` are the JS iterator's own methods;
  - `take(n)` is `take(n)`, and `skip(n)` is `drop(n)`;
  - `for x in it` is `for (const x of $iterator(..))`.
- **Anything that needs every item takes them as an array first:** `collect`
  is `.toArray()`, `count()` is `.toArray().length`, and `max`, `min`,
  `last` and `position` do what ADR 0036 does with an array. Rust would
  also run through the whole iterator for these.
- **A generic `next` may box its `Some`** (ADR 0051). `$iterator(it, next,
  true)` unboxes each item, so an iterator of `()` or of `Option`s still
  yields every item.
- **An impl's `type Item` is allowed.** rustc works out what it is, so it
  needs no JS.
- **Still errors:**
  - `DoubleEndedIterator` and other iterator traits, so no `rev()`;
  - `next()` on a generic `T: Iterator` (ADR 0061 has the rest of generic iterators);
  - std adapters' own `next()` on one of these.

## Why

- **It's lazy where Rust is lazy.** An endless iterator works with `take`,
  `find` and `for` with a `break`, as in Rust.
- **Arrays keep their ADR 0036 output.** Only a chain that starts from the
  crate's own iterator changes, and it uses the same method names wherever
  JS has them.
- **Nothing to add to JS.** `$iterator` is a few lines around
  `Iterator.from`.

## Alternatives

- **Collect into an array at the start.** That's simpler and keeps every
  adapter as it is, but an endless iterator would never finish, and a
  `find` would run `next` past what it needs.
- **A generator per impl** (`function* ()`), which would be as lazy. But
  `next` is what Rust code calls directly, so it has to exist anyway, and
  a generator would be a second copy of it.

## Consequences

- The generated JS needs iterator helpers (`Iterator.from`, `.take`,
  `.drop`, `.toArray`). Current browsers, Node 22 and Bun all have them.
- `for` over one is a `for .. of` on a JS iterator, which calls `next` once
  per item, as Rust does.
