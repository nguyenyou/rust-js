
function $parseF32(s) {
  const read = $parseF64(s);
  const d = read._0;
  if (read.TAG === "Err" || !Number.isFinite(d) || Math.fround(d) === d) {
    return read.TAG === "Err" ? read : { TAG: "Ok", _0: Math.fround(d) };
  }
  const rounded = Math.fround(d);
  // The `f32` this side of `d`: the largest where it rounds to infinity.
  const near = Number.isFinite(rounded) ? rounded : Math.sign(d) * 3.4028234663852886e38;
  const size = Math.abs(near);
  // The `f32` on `d`'s other side, and the tie between them: past the
  // largest, where infinity is, it's as far above as the one below is.
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat32(0, size);
  const bits = view.getUint32(0);
  view.setUint32(0, Math.abs(d) > size ? bits + 1 : bits - 1);
  const other = view.getFloat32(0);
  view.setUint32(0, bits - 1);
  const tie = other === Infinity ? size + (size - view.getFloat32(0)) / 2 : (size + other) / 2;
  if (Math.abs(d) !== tie) return { TAG: "Ok", _0: rounded };
  // The digits, `digits` times 10 to `exponent`, against the tie, `m`
  // times 2 to `k`, in integers.
  const [mantissa, power = "0"] = s.toLowerCase().replace(/^[+-]/, "").split("e");
  const [whole, fraction = ""] = mantissa.split(".");
  const digits = BigInt(whole + fraction || "0");
  const exponent = Number(power) - fraction.length;
  view.setFloat64(0, tie);
  const tieBits = view.getBigUint64(0);
  const m = (tieBits & ((1n << 52n) - 1n)) | (1n << 52n);
  const k = Number(tieBits >> 52n) - 1075;
  const left = digits * 10n ** BigInt(Math.max(exponent, 0)) * 2n ** BigInt(Math.max(-k, 0));
  const right = m * 2n ** BigInt(Math.max(k, 0)) * 10n ** BigInt(Math.max(-exponent, 0));
  // The tie itself goes to the even one, as `Math.fround` takes it.
  if (left === right) return { TAG: "Ok", _0: rounded };
  const chosen = left > right ? Math.max(size, other) : Math.min(size, other);
  return { TAG: "Ok", _0: d < 0 ? -chosen : chosen };
}
