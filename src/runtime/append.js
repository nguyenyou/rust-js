
function $append(items, other) {
  for (const item of other) {
    items.push(item);
  }
  other.length = 0;
}
