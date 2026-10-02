
function $charRange(start, end, inclusive = false) {
  const items = [];
  const last = end.codePointAt(0) - (inclusive ? 0 : 1);
  for (let c = start.codePointAt(0); c <= last; c++) {
    if (c < 0xd800 || c > 0xdfff) {
      items.push(String.fromCodePoint(c));
    }
  }
  return items;
}
