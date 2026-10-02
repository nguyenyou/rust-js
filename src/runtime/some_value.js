
// What's in a `$some`: the box one level shallower.
function $someValue(x) {
  if (x != null && typeof x === "object" && "$someNone" in x) {
    return x.$someNone === 0 ? undefined : { $someNone: x.$someNone - 1 };
  }
  return x;
}
