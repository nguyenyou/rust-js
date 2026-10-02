
function $f64ToBig(x, min, max) {
  if (Number.isNaN(x)) return 0n;
  if (x <= Number(min)) return min;
  if (x >= Number(max)) return max;
  return BigInt(Math.trunc(x));
}
