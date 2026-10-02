
function $orInsert(map, key, value) {
  if (!map.has(key)) {
    map.set(key, value);
  }
  return map.get(key);
}
