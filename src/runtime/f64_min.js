
function $f64Min(a, b) {
  return Number.isNaN(a) ? b : Number.isNaN(b) ? a : Math.min(a, b);
}
