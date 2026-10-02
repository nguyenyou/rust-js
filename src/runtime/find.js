
function $find(s, pattern) {
  const at = s.indexOf(pattern);
  return at === -1 ? undefined : $byteLen(s.slice(0, at));
}
