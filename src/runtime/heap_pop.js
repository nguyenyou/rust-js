
function $heapPop(heap, cmp) {
  if (heap.length === 0) {
    return undefined;
  }
  let top = heap.pop();
  if (heap.length > 0) {
    [top, heap[0]] = [heap[0], top];
    const end = heap.length;
    const item = heap[0];
    let pos = 0;
    let child = 1;
    while (child <= end - 2) {
      if (cmp(heap[child], heap[child + 1]) <= 0) {
        child += 1;
      }
      heap[pos] = heap[child];
      pos = child;
      child = 2 * pos + 1;
    }
    if (child === end - 1) {
      heap[pos] = heap[child];
      pos = child;
    }
    heap[pos] = item;
    $siftUp(heap, 0, pos, cmp);
  }
  return top;
}
