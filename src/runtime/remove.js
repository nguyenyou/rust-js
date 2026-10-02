
function $remove(map, key) {
  const old = map.get(key);
  map.delete(key);
  return old;
}
