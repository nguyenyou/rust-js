
function $zeroPad(text, width) {
  const head = /^[+-]?(0[xbo])?/.exec(text)[0];
  return head + text.slice(head.length).padStart(width - head.length, "0");
}
