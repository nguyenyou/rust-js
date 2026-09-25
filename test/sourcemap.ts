// A minimal source map reader: decode the "mappings" string (base64 VLQ)
// into [jsLine, jsCol, srcLine, srcCol] segments, all 0-based.

const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

export type Segment = { jsLine: number; jsCol: number; srcLine: number; srcCol: number };

export function decodeMappings(mappings: string): Segment[] {
  const segments: Segment[] = [];
  let srcLine = 0, srcCol = 0, source = 0, name = 0;
  mappings.split(";").forEach((line, jsLine) => {
    let jsCol = 0;
    for (const seg of line.split(",")) {
      if (seg === "") continue;
      const fields: number[] = [];
      let value = 0, shift = 0;
      for (const ch of seg) {
        const digit = BASE64.indexOf(ch);
        value += (digit & 31) << shift;
        if (digit & 32) {
          shift += 5;
        } else {
          fields.push(value & 1 ? -(value >>> 1) : value >>> 1);
          value = 0;
          shift = 0;
        }
      }
      jsCol += fields[0];
      if (fields.length >= 4) {
        source += fields[1];
        srcLine += fields[2];
        srcCol += fields[3];
        if (fields.length >= 5) name += fields[4];
        segments.push({ jsLine, jsCol, srcLine, srcCol });
      }
    }
  });
  return segments;
}

/// The Rust position a JS position maps to: the closest segment at or before it.
export function lookup(segments: Segment[], jsLine: number, jsCol: number): Segment | undefined {
  return segments.filter((s) => s.jsLine === jsLine && s.jsCol <= jsCol).at(-1);
}
