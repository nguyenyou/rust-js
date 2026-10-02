
function $countOnes(x) {
  let ones = 0;
  x >>>= 0;
  while (x !== 0) {
    ones += x & 1;
    x >>>= 1;
  }
  return ones;
}
