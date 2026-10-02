
function $pow(base, exp) {
  let result = 1;
  while (exp > 0) {
    if (exp & 1) {
      result = Math.imul(result, base);
    }
    base = Math.imul(base, base);
    exp >>>= 1;
  }
  return result;
}
