
function $rangeNextBack(range) {
  return range.start < range.end ? --range.end : undefined;
}
