
function $fromU32(n) {
  return n > 0x10ffff || (n >= 0xd800 && n <= 0xdfff) ? undefined : String.fromCodePoint(n);
}
