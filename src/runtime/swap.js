
function $swap(v, a, b) {
  if (a >= v.length || b >= v.length) {
    throw new Error(`index out of bounds: the len is ${v.length} but the index is ${Math.max(a, b)}`);
  }
  [v[a], v[b]] = [v[b], v[a]];
}
