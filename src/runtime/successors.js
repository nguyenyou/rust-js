
function* $successors(next, successor, boxed = false) {
  while (next != null) {
    const item = boxed ? $someValue(next) : next;
    next = successor(item);
    yield item;
  }
}
