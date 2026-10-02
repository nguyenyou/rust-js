
function $insertAt(v, index, item) {
  if (index > v.length) {
    throw new Error(`insertion index (is ${index}) should be <= len (is ${v.length})`);
  }
  v.splice(index, 0, item);
}
