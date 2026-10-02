
function $nextIf(it, f) {
  return it.at < it.items.length && f(it.items[it.at]) ? it.items[it.at++] : undefined;
}
