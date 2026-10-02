
function $powi(x, n) {
  const reciprocal = n < 0;
  let result = 1;
  while (true) {
    if (n & 1) {
      result *= x;
    }
    n = (n / 2) | 0;
    if (n === 0) {
      break;
    }
    x *= x;
  }
  return reciprocal ? 1 / result : result;
}
