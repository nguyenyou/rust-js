
function $siftUp(heap, start, pos, cmp) {
  const item = heap[pos];
  while (pos > start) {
    const parent = (pos - 1) >>> 1;
    if (cmp(item, heap[parent]) <= 0) {
      break;
    }
    heap[pos] = heap[parent];
    pos = parent;
  }
  heap[pos] = item;
  return pos;
}
