
function $removeAt(v, index) {
  if (index >= v.length) {
    throw new Error(`removal index (is ${index}) should be < len (is ${v.length})`);
  }
  return v.splice(index, 1)[0];
}
