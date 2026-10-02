
function $try(f) {
  try {
    return { TAG: "Ok", _0: f() };
  } catch (e) {
    return { TAG: "Err", _0: e };
  }
}
