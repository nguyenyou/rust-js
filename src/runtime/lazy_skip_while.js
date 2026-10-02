
function* $lazySkipWhile(items, skip) {
  let skipping = true;
  for (const item of items) {
    if (skipping && skip(item)) {
      continue;
    }
    skipping = false;
    yield item;
  }
}
