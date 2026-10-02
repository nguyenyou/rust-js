
function $repeat(item, count, clone) {
  const result = [];
  if (count > 0) {
    for (let i = 1; i < count; i++) result.push(clone(item));
    result.push(item);
  }
  return result;
}
