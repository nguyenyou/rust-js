
function $replace(s, pattern, replacement) {
  if (pattern !== "") return s.replaceAll(pattern, replacement);
  return Array.from(s, (c) => replacement + c).join("") + replacement;
}
function $split(s, pattern) {
  return pattern === "" ? ["", ...s, ""] : s.split(pattern);
}
