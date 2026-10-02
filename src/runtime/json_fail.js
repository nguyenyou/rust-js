
class $JsonError extends Error {
  constructor(message, line = 0, column = 0) {
    super(message);
    this.line = line;
    this.column = column;
  }
}
function $jsonError(message) {
  return new $JsonError(message);
}
// A float as serde_json writes one: its shortest digits, fixed from 1e-5
// to 1e15, else `1.5e+16`.
function $jsonNumber(x) {
  if (!Number.isFinite(x)) return "null";
  if (x === 0) return Object.is(x, -0) ? "-0.0" : "0.0";
  const [mantissa, exponent] = x.toExponential().split("e");
  const e = Number(exponent);
  if (e < -5 || e > 15) return `${mantissa}e${e < 0 ? "-" : "+"}${Math.abs(e)}`;
  const fixed = String(x);
  return fixed.includes(".") ? fixed : `${fixed}.0`;
}
