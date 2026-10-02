
function $charIndices(s) {
  const indices = [];
  let at = 0;
  for (const c of s) {
    indices.push([at, c]);
    at += $byteLen(c);
  }
  return indices;
}
