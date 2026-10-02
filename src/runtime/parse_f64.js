
function $parseF64(s) {
  if (s === "") return { TAG: "Err", _0: "cannot parse float from empty string" };
  const lower = s.toLowerCase();
  const sign = lower.startsWith("-") ? -1 : 1;
  const rest = lower.replace(/^[+-]/, "");
  if (rest === "inf" || rest === "infinity") return { TAG: "Ok", _0: sign * Infinity };
  if (rest === "nan") return { TAG: "Ok", _0: NaN };
  if (!/^([0-9]+\.?[0-9]*|\.[0-9]+)(e[+-]?[0-9]+)?$/.test(rest)) return { TAG: "Err", _0: "invalid float literal" };
  return { TAG: "Ok", _0: sign * Number(rest) };
}
