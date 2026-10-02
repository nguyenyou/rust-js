
function $trailingZeros(x, bits) {
  return x === 0 ? bits : 31 - Math.clz32(x & -x);
}
