
function $removeOpt(items, index) {
  return index < items.length ? items.splice(index, 1)[0] : undefined;
}
