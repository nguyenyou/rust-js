
// `collect()` into a `Result`: the first `Err`, where it stops, as Rust's
// does, or `Ok` of all the values.
function $collectResults(items) {
  const values = [];
  for (const item of items) {
    if (item.TAG === "Err") return item;
    values.push(item._0);
  }
  return { TAG: "Ok", _0: values };
}
