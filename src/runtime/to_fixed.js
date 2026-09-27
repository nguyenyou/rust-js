
function $toFixed(value, digits) {
  if (Number.isNaN(value)) return "NaN";
  if (value === Infinity) return "inf";
  if (value === -Infinity) return "-inf";
  const sign = value < 0 || Object.is(value, -0) ? "-" : "";
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, Math.abs(value));
  const bits = view.getBigUint64(0);
  const exponent = Number((bits >> 52n) & 2047n);
  const fraction = bits & ((1n << 52n) - 1n);
  const significand = exponent === 0 ? fraction : fraction | (1n << 52n);
  const shift = exponent === 0 ? -1074 : exponent - 1075;
  // |value| * 10^digits, as a fraction.
  let numerator = significand * 10n ** BigInt(digits);
  let denominator = 1n;
  if (shift >= 0) numerator <<= BigInt(shift);
  else denominator <<= BigInt(-shift);
  let rounded = numerator / denominator;
  const twice = 2n * (numerator % denominator);
  if (twice > denominator || (twice === denominator && rounded % 2n === 1n)) rounded += 1n;
  const text = rounded.toString().padStart(digits + 1, "0");
  return sign + (digits === 0 ? text : text.slice(0, -digits) + "." + text.slice(-digits));
}
