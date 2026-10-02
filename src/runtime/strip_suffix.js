
function $stripSuffix(s, suffix) {
  return s.endsWith(suffix) ? s.slice(0, s.length - suffix.length) : undefined;
}
