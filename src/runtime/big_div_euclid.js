
function $bigDivEuclid(a, b, min) {
  const q = $bigDiv(a, b, min);
  if (a % b < 0n) return b > 0n ? q - 1n : q + 1n;
  return q;
}
