
function $rfind(s, pattern) {
  const at = s.lastIndexOf(pattern);
  return at === -1 ? undefined : $byteLen(s.slice(0, at));
}
