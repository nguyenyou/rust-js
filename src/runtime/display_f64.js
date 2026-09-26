// Shortest round-trip decimal, using exact rational arithmetic.
// Rust Display uses decimal notation, including for very small/large values.
// Not JS's `String(x)`, though its digits are also the shortest: when two are
// equally close, JS takes the even one (1888570120608320.2), and Rust the
// larger (1888570120608320.3).
function $displayF64(value) {
  if (Number.isNaN(value)) return 'NaN';
  if (value === Infinity) return 'inf';
  if (value === -Infinity) return '-inf';
  const sign = value < 0 || Object.is(value, -0) ? '-' : '';
  const x = Math.abs(value);
  if (x === 0) return sign + '0';

  const bitsView = new DataView(new ArrayBuffer(8));
  bitsView.setFloat64(0, x);
  const bits = bitsView.getBigUint64(0);
  const exponentBits = Number((bits >> 52n) & 2047n);
  const fraction = bits & ((1n << 52n) - 1n);
  const significand = exponentBits === 0 ? fraction : fraction + (1n << 52n);
  const binaryExponent = exponentBits === 0 ? -1074 : exponentBits - 1023 - 52;
  const numerator = binaryExponent >= 0 ? significand << BigInt(binaryExponent) : significand;
  const denominator = binaryExponent < 0 ? 1n << BigInt(-binaryExponent) : 1n;

  // The floating-point logarithm is only an estimate; correct it exactly.
  let exponent = Math.floor(Math.log10(x));
  const atLeastPowerOfTen = e => e >= 0
    ? numerator >= denominator * 10n ** BigInt(e)
    : numerator * 10n ** BigInt(-e) >= denominator;
  while (!atLeastPowerOfTen(exponent)) exponent--;
  while (atLeastPowerOfTen(exponent + 1)) exponent++;

  for (let precision = 1; precision <= 17; precision++) {
    const power = exponent - precision + 1;
    const n = power < 0 ? numerator * 10n ** BigInt(-power) : numerator;
    const d = power > 0 ? denominator * 10n ** BigInt(power) : denominator;
    const floor = n / d;
    const rounded = floor + (2n * (n % d) >= d ? 1n : 0n);
    // The rounding interval can be asymmetric at a binary power boundary.
    let best;
    let bestDistance;
    for (const candidate of [rounded, rounded - 1n, rounded + 1n]) {
      if (candidate > 0n && Number(`${candidate}e${power}`) === x) {
        const delta = candidate * d - n;
        const distance = delta < 0n ? -delta : delta;
        if (best === undefined || distance < bestDistance || (distance === bestDistance && candidate > best)) {
          best = candidate;
          bestDistance = distance;
        }
      }
    }
    if (best !== undefined) {
      let digits = best.toString();
      let decimalPower = power;
      while (digits.endsWith('0')) {
        digits = digits.slice(0, -1);
        decimalPower++;
      }
      const point = digits.length + decimalPower;
      const body = point <= 0 ? '0.' + '0'.repeat(-point) + digits
        : point >= digits.length ? digits + '0'.repeat(point - digits.length)
        : digits.slice(0, point) + '.' + digits.slice(point);
      return sign + body;
    }
  }
  throw new Error('No f64 round-trip decimal found');
}
