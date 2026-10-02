
function $rest(it) {
  const rest = it.items.slice(it.at);
  it.at = it.items.length;
  return rest;
}
