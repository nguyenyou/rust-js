
function $displayJsonError(e) {
  return e.line === 0 ? e.message : `${e.message} at line ${e.line} column ${e.column}`;
}
function $debugJsonError(e) {
  return `Error(${$debugStr(e.message)}, line: ${e.line}, column: ${e.column})`;
}
