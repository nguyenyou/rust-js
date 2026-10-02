
function $scan(items, init, f) {
  const state = { value: init };
  const out = [];
  for (const item of items) {
    const next = f(state, item);
    if (next === undefined) {
      break;
    }
    out.push(next);
  }
  return out;
}
