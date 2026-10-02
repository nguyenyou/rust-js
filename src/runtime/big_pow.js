
function $bigPow(base, exp) {
  let result = 1n;
  base = BigInt.asUintN(64, base);
  for (let e = exp; e > 0; e >>>= 1) {
    if (e & 1) result = BigInt.asUintN(64, result * base);
    base = BigInt.asUintN(64, base * base);
  }
  return result;
}
