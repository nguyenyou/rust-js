
function $toDigit(c, radix) {
  const digit = parseInt(c, 36);
  return digit < radix ? digit : undefined;
}
