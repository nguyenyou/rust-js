
function $totalCmp(a, b) {
  const view = new DataView(new ArrayBuffer(16));
  view.setFloat64(0, a);
  view.setFloat64(8, b);
  const ordered = (bits) => bits ^ BigInt.asIntN(64, BigInt.asUintN(64, bits >> 63n) >> 1n);
  const x = ordered(view.getBigInt64(0));
  const y = ordered(view.getBigInt64(8));
  return x < y ? -1 : x > y ? 1 : 0;
}
