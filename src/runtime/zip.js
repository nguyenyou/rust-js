
function $zip(a, b) {
  return Array.from({ length: Math.min(a.length, b.length) }, (_, i) => [a[i], b[i]]);
}
