
function $position(items, found) {
  let i = 0;
  for (const item of items) {
    if (found(item)) return i;
    i++;
  }
  return undefined;
}
