
function $parseBig(s, min, max) {
  const error = (message) => ({ TAG: "Err", _0: message });
  if (s === "") return error("cannot parse integer from empty string");
  if (!(min < 0n ? /^[+-]?[0-9]+$/ : /^\+?[0-9]+$/).test(s)) return error("invalid digit found in string");
  const n = BigInt(s);
  if (n > max) return error("number too large to fit in target type");
  if (n < min) return error("number too small to fit in target type");
  return { TAG: "Ok", _0: n };
}
