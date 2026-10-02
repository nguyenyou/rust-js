
function* $repeating(value, clone = (value) => value) {
  while (true) {
    yield clone(value);
  }
}
