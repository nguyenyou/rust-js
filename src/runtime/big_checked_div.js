
function $bigCheckedDiv(a, b, min) {
  return b === 0n || (a === min && b === -1n) ? undefined : a / b;
}
