
function $f64ToInt(x, min, max) {
  if (Number.isNaN(x)) return 0;
  return Math.max(min, Math.min(max, Math.trunc(x))) + 0;
}
