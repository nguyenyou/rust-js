
function $partition(items, keep) {
  const yes = [];
  const no = [];
  for (const item of items) {
    (keep(item) ? yes : no).push(item);
  }
  return [yes, no];
}
