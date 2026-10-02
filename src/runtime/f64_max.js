
function $f64Max(a, b) {
  return Number.isNaN(a) ? b : Number.isNaN(b) ? a : Math.max(a, b);
}
