
function $maxBy(items, cmp, boxed = false) {
  if (items.length === 0) {
    return undefined;
  }
  const max = items.reduce((max, x) => (cmp(x, max) >= 0 ? x : max));
  return boxed ? $some(max) : max;
}
