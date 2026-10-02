
function $bigRemEuclid(a, b, min) {
  const r = $bigRem(a, b, min);
  return r < 0n ? (b < 0n ? r - b : r + b) : r;
}
