
function $thenCmp(...orders) {
  for (const order of orders) {
    if (order !== 0) {
      return order;
    }
  }
  return 0;
}
