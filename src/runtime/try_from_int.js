
function $tryFromInt(x, lo, hi) {
  if (x < lo) return { TAG: "Err", _0: "NegOverflow" };
  if (x > hi) return { TAG: "Err", _0: "PosOverflow" };
  return { TAG: "Ok", _0: typeof hi === "bigint" ? BigInt(x) : Number(x) };
}
