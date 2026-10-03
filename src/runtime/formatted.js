
// A `Formatter`'s options, applied to the text a number, a `bool` or a
// string shows as std's `fmt`s apply them (ADR 0058): a number takes a
// sign and zeros, a string is cut to its precision, and either is padded.
function $formatted(text, options, numeric = false) {
  if (options === undefined) return text;
  const { width, precision } = options;
  if (numeric && options.plus) text = $plus(text);
  if (!numeric && precision !== undefined) text = [...text].slice(0, precision).join("");
  if (width === undefined) return text;
  if (numeric && options.zero) return $zeroPad(text, width);
  return $pad(text, width, options.align ?? (numeric ? ">" : "<"), options.fill);
}
