
function $debugF64(value) {
  const size = Math.abs(value);
  if (Number.isFinite(value) && size !== 0 && (size < 1e-4 || size >= 1e16)) {
    return value.toExponential().replace("e+", "e");
  }
  const text = $displayF64(value);
  return Number.isFinite(value) && !text.includes(".") ? text + ".0" : text;
}
