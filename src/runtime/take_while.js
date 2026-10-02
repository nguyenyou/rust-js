
function $takeWhile(items, keep) {
  const end = items.findIndex((item) => !keep(item));
  return end < 0 ? items.slice() : items.slice(0, end);
}
