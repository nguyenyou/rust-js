
function $parseBool(s) {
  return s === "true" || s === "false"
    ? { TAG: "Ok", _0: s === "true" }
    : { TAG: "Err", _0: "provided string was not `true` or `false`" };
}
