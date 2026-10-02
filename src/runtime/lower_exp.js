
function $lowerExp(x) {
  if (Number.isNaN(x)) return "NaN";
  if (!Number.isFinite(x)) return x > 0 ? "inf" : "-inf";
  return x.toExponential().replace("e+", "e");
}
