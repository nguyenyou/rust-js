
function $heapFrom(items, cmp) {
  const heap = items.slice();
  let n = heap.length >>> 1;
  while (n > 0) {
    n -= 1;
    $siftDown(heap, n, heap.length, cmp);
  }
  return heap;
}
