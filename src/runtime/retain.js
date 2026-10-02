
function $retain(v, keep) {
  let n = 0;
  for (const x of v) {
    if (keep(x)) {
      v[n++] = x;
    }
  }
  v.length = n;
}
