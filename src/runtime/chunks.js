
function $chunks(v, size) {
  if (size === 0) {
    throw new Error("chunk size must be non-zero");
  }
  return Array.from({ length: Math.ceil(v.length / size) }, (_, i) => v.slice(i * size, i * size + size));
}
