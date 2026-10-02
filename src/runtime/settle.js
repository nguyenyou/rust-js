
function $settle(promise) {
  return promise.then((value) => ({ TAG: "Ok", _0: value }), (e) => ({ TAG: "Err", _0: e }));
}
