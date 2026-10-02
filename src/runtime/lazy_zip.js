
function* $lazyZip(a, b) {
  const right = b[Symbol.iterator]();
  for (const item of a) {
    const other = right.next();
    if (other.done) {
      return;
    }
    yield [item, other.value];
  }
}
