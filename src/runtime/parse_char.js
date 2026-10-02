
function $parseChar(s) {
  const chars = [...s];
  if (chars.length === 1) return { TAG: "Ok", _0: s };
  return { TAG: "Err", _0: chars.length === 0 ? "cannot parse char from empty string" : "too many characters in string" };
}
