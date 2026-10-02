
function $bigClamp(value, lo, hi) {
  return value < lo ? lo : value > hi ? hi : value;
}
