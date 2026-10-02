
function $min(items) {
  return items.length === 0 ? undefined : items.reduce((min, x) => (x < min ? x : min));
}
