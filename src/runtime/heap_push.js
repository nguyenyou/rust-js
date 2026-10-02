
function $heapPush(heap, item, cmp) {
  heap.push(item);
  $siftUp(heap, 0, heap.length - 1, cmp);
}
