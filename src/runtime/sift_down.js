
function $siftDown(heap, pos, end, cmp) {
  const item = heap[pos];
  let child = 2 * pos + 1;
  while (child <= end - 2) {
    if (cmp(heap[child], heap[child + 1]) <= 0) {
      child += 1;
    }
    if (cmp(item, heap[child]) >= 0) {
      heap[pos] = item;
      return pos;
    }
    heap[pos] = heap[child];
    pos = child;
    child = 2 * pos + 1;
  }
  if (child === end - 1 && cmp(item, heap[child]) < 0) {
    heap[pos] = heap[child];
    pos = child;
  }
  heap[pos] = item;
  return pos;
}
