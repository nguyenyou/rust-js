
function* $repeatingWith(f) {
  while (true) {
    yield f();
  }
}
