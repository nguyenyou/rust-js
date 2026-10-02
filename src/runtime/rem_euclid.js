
function $remEuclid(a, b, min) {
  const r = $rem(a, b, min) + 0;
  return r < 0 ? (b < 0 ? r - b : r + b) : r;
}
