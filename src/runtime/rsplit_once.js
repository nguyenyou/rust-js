
function $rsplitOnce(s, separator) {
  const i = s.lastIndexOf(separator);
  return i < 0 ? undefined : [s.slice(0, i), s.slice(i + separator.length)];
}
