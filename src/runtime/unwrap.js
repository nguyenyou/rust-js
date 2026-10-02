
function $unwrap(value, message = "called `Option::unwrap()` on a `None` value") {
  if (value == null) {
    throw new Error(message);
  }
  return value;
}
