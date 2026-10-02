
function $heapSorted(items, cmp) {
  const heap = items.slice();
  let end = heap.length;
  while (end > 1) {
    end -= 1;
    [heap[0], heap[end]] = [heap[end], heap[0]];
    $siftDown(heap, 0, end, cmp);
  }
  return heap;
}
