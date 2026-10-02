
function $plus(text) {
  return text.startsWith("-") || text === "NaN" ? text : "+" + text;
}
