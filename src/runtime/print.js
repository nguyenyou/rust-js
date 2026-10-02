
const $printed = ["", ""];
function $print(text) {
  $write(text, 0);
}
function $eprint(text) {
  $write(text, 1);
}
function $write(text, error) {
  const stream = globalThis.process?.[error ? "stderr" : "stdout"];
  if (stream?.write) {
    stream.write(text);
    return;
  }
  const log = error ? console.error : console.log;
  const lines = ($printed[error] + text).split("\n");
  const rest = lines.pop();
  for (const line of lines) log(line);
  // What's left unended is written when this task ends, so none is lost.
  if (rest !== "" && $printed[error] === "") {
    queueMicrotask(() => {
      if ($printed[error] !== "") log($printed[error]);
      $printed[error] = "";
    });
  }
  $printed[error] = rest;
}
