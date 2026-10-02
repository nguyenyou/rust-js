
function $sortedEntries(map, cmp) {
  return Array.from(map).sort((a, b) => cmp(a[0], b[0]));
}
