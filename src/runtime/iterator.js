
// A Rust iterator of the crate's own as a JS one: its `next` returns the
// item, or `None` at the end. JS's iterator helpers are lazy, like Rust's
// adapters, so an endless one is fine until something wants all of it. A
// generic `next` may box its `Some` (ADR 0051): `boxed` unboxes it.
function $iterator(iterator, next, boxed = false) {
  return Iterator.from({
    next() {
      const item = next(iterator);
      if (item == null) {
        return { done: true, value: undefined };
      }
      return { done: false, value: boxed ? $someValue(item) : item };
    }
  });
}
