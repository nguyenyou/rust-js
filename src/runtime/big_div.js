
function $bigDiv(a, b, min) {
  if (b === 0n) {
    throw new Error("attempt to divide by zero");
  }
  if (a === min && b === -1n) {
    throw new Error("attempt to divide with overflow");
  }
  return a / b;
}
