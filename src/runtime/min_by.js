
function $minBy(items, cmp, boxed = false) {
  if (items.length === 0) {
    return undefined;
  }
  const min = items.reduce((min, x) => (cmp(x, min) < 0 ? x : min));
  return boxed ? $some(min) : min;
}
