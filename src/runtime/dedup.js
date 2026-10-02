
function $dedup(v) {
  let n = 0;
  for (const item of v) {
    if (n === 0 || v[n - 1] !== item) {
      v[n++] = item;
    }
  }
  v.length = n;
}
