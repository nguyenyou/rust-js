
// `left` and `right` are their `{:?}` strings already (ADR 0060).
function $assertFailed(kind, left, right, message) {
  const op = kind === "Eq" ? "==" : kind === "Ne" ? "!=" : "matches";
  const why = message === undefined ? "" : ": " + message;
  throw new Error("assertion `left " + op + " right` failed" + why + "\n  left: " + left + "\n right: " + right);
}
