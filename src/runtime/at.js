
function $at(items, index) {
  if (index < 0 || index >= items.length) throw new Error(`index out of bounds: the len is ${items.length} but the index is ${index}`);
  return index;
}
