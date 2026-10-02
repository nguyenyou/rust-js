
function $splitOnce(s, separator) {
  const i = s.indexOf(separator);
  return i < 0 ? undefined : [s.slice(0, i), s.slice(i + separator.length)];
}
