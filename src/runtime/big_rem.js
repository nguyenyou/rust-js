
function $bigRem(a, b, min) {
  if (b === 0n) {
    throw new Error("attempt to calculate the remainder with a divisor of zero");
  }
  if (a === min && b === -1n) {
    throw new Error("attempt to calculate the remainder with overflow");
  }
  return a % b;
}
