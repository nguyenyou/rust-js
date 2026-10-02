
function $checkedPow(base, exp, lo, hi) {
  const fits = (x) => x >= lo && x <= hi;
  const result = (x) => (typeof hi === "bigint" ? x : Number(x));
  if (exp === 0) return result(1n);
  let b = BigInt(base);
  let acc = 1n;
  while (exp > 1) {
    if (exp & 1) {
      acc *= b;
      if (!fits(acc)) return undefined;
    }
    exp >>>= 1;
    b *= b;
    if (!fits(b)) return undefined;
  }
  acc *= b;
  return fits(acc) ? result(acc) : undefined;
}
