
function* $lazyTakeWhile(items, keep) {
  for (const item of items) {
    if (!keep(item)) {
      return;
    }
    yield item;
  }
}
