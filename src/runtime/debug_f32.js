
function $debugF32(value) {
  const size = Math.abs(value);
  if (Number.isFinite(value) && size !== 0 && (size < Math.fround(1e-4) || size >= Math.fround(1e16))) {
    const [digits, point] = $f32Digits(size);
    const mantissa = digits.length > 1 ? digits[0] + "." + digits.slice(1) : digits;
    return (value < 0 ? "-" : "") + mantissa + "e" + (point - 1);
  }
  const text = $displayF32(value);
  return Number.isFinite(value) && !text.includes(".") ? text + ".0" : text;
}
