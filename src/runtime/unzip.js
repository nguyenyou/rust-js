
function $unzip(pairs) {
  return [pairs.map(([a]) => a), pairs.map(([, b]) => b)];
}
