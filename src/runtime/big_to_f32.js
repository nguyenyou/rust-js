
function $bigToF32(n) {
  const negative = n < 0n;
  let size = negative ? -n : n;
  const length = size.toString(2).length;
  if (length > 24) {
    const shift = BigInt(length - 24);
    const rest = size & ((1n << shift) - 1n);
    const half = 1n << (shift - 1n);
    size >>= shift;
    if (rest > half || (rest === half && (size & 1n) === 1n)) size++;
    size <<= shift;
  }
  const x = Number(size);
  return negative ? -x : x;
}
