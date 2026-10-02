
function $range(start, end) {
  return Array.from({ length: Math.max(0, end - start) }, (_, i) => start + i);
}
