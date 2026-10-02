
function $powiF32(x, n) {
  const reciprocal = n < 0;
  let result = 1;
  while (true) {
    if (n & 1) {
      result = Math.fround(result * x);
    }
    n = (n / 2) | 0;
    if (n === 0) {
      break;
    }
    x = Math.fround(x * x);
  }
  return reciprocal ? Math.fround(1 / result) : result;
}
