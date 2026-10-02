
function $bigCountOnes(x) {
  return BigInt.asUintN(64, x).toString(2).replaceAll("0", "").length;
}
function $bigLeadingZeros(x) {
  const bits = BigInt.asUintN(64, x);
  return bits === 0n ? 64 : 64 - bits.toString(2).length;
}
function $bigTrailingZeros(x) {
  const bits = BigInt.asUintN(64, x).toString(2);
  return x === 0n ? 64 : bits.length - 1 - bits.lastIndexOf("1");
}
