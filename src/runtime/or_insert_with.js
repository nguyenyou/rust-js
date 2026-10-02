
function $orInsertWith(map, key, make) {
  if (!map.has(key)) {
    map.set(key, make());
  }
  return map.get(key);
}
