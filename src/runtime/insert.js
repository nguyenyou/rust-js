
function $insert(map, key, value) {
  const old = map.get(key);
  map.set(key, value);
  return old;
}
