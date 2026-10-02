
function $fromDigit(num, radix) {
  if (radix > 36) throw new Error("from_digit: radix is too high (maximum 36)");
  return num < radix ? String.fromCharCode(num < 10 ? 48 + num : 87 + num) : undefined;
}
