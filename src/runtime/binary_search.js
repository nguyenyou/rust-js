
function $binarySearch(items, x) {
  let size = items.length;
  if (size === 0) {
    return { TAG: "Err", _0: 0 };
  }
  let base = 0;
  while (size > 1) {
    const half = size >>> 1;
    const mid = base + half;
    if (!(items[mid] > x)) {
      base = mid;
    }
    size -= half;
  }
  const found = items[base];
  return found === x ? { TAG: "Ok", _0: base } : { TAG: "Err", _0: base + (found < x ? 1 : 0) };
}
