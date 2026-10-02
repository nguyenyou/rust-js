
function $pad(text, width, align, fill = " ") {
  const room = width - [...text].length;
  if (room <= 0) return text;
  const before = align === ">" ? room : align === "^" ? Math.floor(room / 2) : 0;
  return fill.repeat(before) + text + fill.repeat(room - before);
}
