
function $checkedDiv(a, b, min) {
  return b === 0 || (a === min && b === -1) ? undefined : Math.trunc(a / b) + 0;
}
