
function $unwrapOk(result, message = "called `Result::unwrap()` on an `Err` value", debug = $debug) {
  if (result.TAG === "Err") {
    throw new Error(message + ": " + debug(result._0));
  }
  return result._0;
}
