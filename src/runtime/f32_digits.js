// The shortest digits of the positive `f32` `x` that read back as `x`, the
// nearest to it of those, as Rust's: `[digits, point]`, `x` being `0.digits`
// times 10 to `point` (ADR 0122). A decimal reads back as `x` when it's in
// `x`'s rounding interval, at an end of it only when `x`'s significand is
// even, as a tie rounds to an even one. Tested exactly: through an `f64`,
// `Math.fround(Number(text))` rounds twice, which can miss `x`.
function $f32Digits(x) {
  const view = new DataView(new ArrayBuffer(4));
  view.setFloat32(0, x);
  const bits = view.getUint32(0);
  const exponentBits = bits >>> 23;
  const fraction = bits & 0x7fffff;
  const significand = BigInt(exponentBits === 0 ? fraction : fraction | 0x800000);
  const exponent2 = exponentBits === 0 ? -149 : exponentBits - 150;
  // In quarters of the last place: `x` is 4m of them, and its interval's ends
  // 2 either side, or 1 below a power of two, where the gap below is half.
  const quarters = 4n * significand;
  const below = fraction === 0 && exponentBits > 1 ? 1n : 2n;
  const even = (significand & 1n) === 0n;
  const quarterNum = exponent2 >= 2 ? 1n << BigInt(exponent2 - 2) : 1n;
  const quarterDen = exponent2 < 2 ? 1n << BigInt(2 - exponent2) : 1n;
  // `c` times 10 to `p`, against `k` quarters.
  const compare = (c, p, k) => {
    const left = p >= 0 ? c * 10n ** BigInt(p) * quarterDen : c * quarterDen;
    const right = p >= 0 ? k * quarterNum : k * quarterNum * 10n ** BigInt(-p);
    return left < right ? -1 : left > right ? 1 : 0;
  };
  const readsBack = (c, p) => {
    const low = compare(c, p, quarters - below);
    const high = compare(c, p, quarters + 2n);
    return (low > 0 || (low === 0 && even)) && (high < 0 || (high === 0 && even));
  };

  // The first digit's power of ten, estimated, then made exact.
  let exponent = Math.floor(Math.log10(x));
  while (compare(1n, exponent, quarters) > 0) exponent--;
  while (compare(1n, exponent + 1, quarters) <= 0) exponent++;

  for (let precision = 1; precision <= 9; precision++) {
    const power = exponent - precision + 1;
    // `x` over 10 to `power`, as `n / d`, and the integer nearest it.
    const n = power >= 0 ? quarters * quarterNum : quarters * quarterNum * 10n ** BigInt(-power);
    const d = power >= 0 ? quarterDen * 10n ** BigInt(power) : quarterDen;
    const rounded = n / d + (2n * (n % d) >= d ? 1n : 0n);
    let best;
    let bestDistance;
    for (const candidate of [rounded, rounded - 1n, rounded + 1n]) {
      if (candidate > 0n && readsBack(candidate, power)) {
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
      while (digits.endsWith("0")) {
        digits = digits.slice(0, -1);
        decimalPower++;
      }
      return [digits, digits.length + decimalPower];
    }
  }
  throw new Error("No f32 round-trip decimal found");
}
