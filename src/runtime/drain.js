
function $drain(items, start, end = items.length) {
  if (start > end) throw new Error(`slice index starts at ${start} but ends at ${end}`);
  if (end > items.length) throw new Error(`range end index ${end} out of range for slice of length ${items.length}`);
  return items.splice(start, end - start);
}
