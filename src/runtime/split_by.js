
function $splitBy(s, matches) {
  const pieces = [""];
  for (const c of s) {
    if (matches(c)) {
      pieces.push("");
    } else {
      pieces[pieces.length - 1] += c;
    }
  }
  return pieces;
}
