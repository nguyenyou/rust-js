
function $debugStr(s, quote = '"') {
  let out = quote;
  for (const c of s) {
    if (c === quote || c === "\\") out += "\\" + c;
    else if (c === "\n") out += "\\n";
    else if (c === "\r") out += "\\r";
    else if (c === "\t") out += "\\t";
    else if (c === "\0") out += "\\0";
    else if (/[\p{Cc}\p{Cf}\p{Cs}\p{Co}\p{Cn}\p{Zl}\p{Zp}\p{Grapheme_Extend}]/u.test(c) || (c !== " " && /\p{Zs}/u.test(c)))
      out += "\\u{" + c.codePointAt(0).toString(16) + "}";
    else out += c;
  }
  return out + quote;
}
