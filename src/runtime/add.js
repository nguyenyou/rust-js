
function $add(set, item) {
  const added = !set.has(item);
  set.add(item);
  return added;
}
