
function $lines(s) {
  const lines = s.split("\n");
  const last = lines.pop();
  const ended = lines.map((line) => (line.endsWith("\r") ? line.slice(0, -1) : line));
  if (last !== "") {
    ended.push(last);
  }
  return ended;
}
