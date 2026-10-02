
function $skipWhile(items, skip) {
  const start = items.findIndex((item) => !skip(item));
  return start < 0 ? [] : items.slice(start);
}
