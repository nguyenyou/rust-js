
function $checked(value, lo, hi) {
  return value >= lo && value <= hi ? value + 0 : undefined;
}
