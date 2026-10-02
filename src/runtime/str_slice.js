
function $strSlice(s, start, end) {
  const length = $byteLen(s);
  end ??= length;
  if (start > length) throw new Error(`start byte index ${start} is out of bounds for string of length ${length}`);
  if (end > length) throw new Error(`end byte index ${end} is out of bounds for string of length ${length}`);
  if (start > end) throw new Error(`byte range starts at ${start} but ends at ${end}`);
  return s.slice($unitAt(s, start, "start"), $unitAt(s, end, "end"));
}

function $unitAt(s, at, which) {
  let bytes = 0;
  let unit = 0;
  for (const c of s) {
    if (bytes === at) return unit;
    const next = bytes + $byteLen(c);
    if (next > at) {
      throw new Error(`${which} byte index ${at} is not a char boundary; it is inside ${$debugStr(c, "'")} (bytes ${bytes}..${next} of string)`);
    }
    bytes = next;
    unit += c.length;
  }
  return unit;
}
