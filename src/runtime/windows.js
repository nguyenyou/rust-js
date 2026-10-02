
function $windows(v, size) {
  if (size === 0) {
    throw new Error("window size must be non-zero");
  }
  return Array.from({ length: Math.max(0, v.length - size + 1) }, (_, i) => v.slice(i, i + size));
}
