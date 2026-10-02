
function $max(items) {
  return items.length === 0 ? undefined : items.reduce((max, x) => (x >= max ? x : max));
}
