
function $asciiCase(text, upper = false) {
  return upper
    ? text.replace(/[a-z]+/g, (letters) => letters.toUpperCase())
    : text.replace(/[A-Z]+/g, (letters) => letters.toLowerCase());
}
