
// `Vec::pop` of a generic `T`: `None` for an empty one, else `Some` of the last.
function $pop(items) {
  return items.length === 0 ? undefined : $some(items.pop());
}
