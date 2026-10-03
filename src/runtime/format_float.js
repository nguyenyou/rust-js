
// A float's precision is its digits; the rest is `$formatted`'s.
function $formatFloat(value, options, show) {
  const precision = options?.precision;
  return $formatted(precision === undefined ? show(value) : $toFixed(value, precision), options, true);
}
