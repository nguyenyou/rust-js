
function $jsonWriter(pretty) {
  // For each array or object open: whether nothing is in it yet.
  const empty = [];
  const next = (json) => {
    const first = empty[empty.length - 1];
    empty[empty.length - 1] = false;
    if (pretty) json.text += (first ? "\n" : ",\n") + "  ".repeat(empty.length);
    else if (!first) json.text += ",";
  };
  const open = (json, bracket) => {
    empty.push(true);
    json.text += bracket;
  };
  const close = (json, bracket) => {
    if (!empty.pop() && pretty) json.text += "\n" + "  ".repeat(empty.length);
    json.text += bracket;
  };
  return {
    text: "",
    null() { this.text += "null"; },
    bool(b) { this.text += String(b); },
    int(n) { this.text += String(n); },
    // An `f32`'s, `single`, with its own digits, as serde_json writes it (ADR 0122).
    number(x, single) { this.text += single ? $jsonF32(x) : $jsonNumber(x); },
    char(c) { this.text += JSON.stringify(c); },
    string(s) { this.text += JSON.stringify(s); },
    // An externally tagged unit variant: its name.
    variant(name) { this.string(name); },
    beginArray() { open(this, "["); },
    beginTuple() { open(this, "["); },
    beginTupleStruct() { open(this, "["); },
    element() { next(this); },
    endArray() { close(this, "]"); },
    beginObject() { open(this, "{"); },
    key(k) {
      next(this);
      this.text += JSON.stringify(k) + (pretty ? ": " : ":");
    },
    endObject() { close(this, "}"); },
    // `#[serde(flatten)]`: `value`'s entries, among the object's own.
    flat(value, write) { write(value, $jsonFlat(this)); },
  };
}
// serde's `FlatMapSerializer`: a struct's or a map's entries go into the
// object `into` is writing; a variant is an entry of its name; `None` and
// `()` are nothing; and anything else can't be flattened.
function $jsonFlat(into) {
  let depth = 0;
  const top = (what) => {
    if (depth === 0) throw $jsonError(`can only flatten structs and maps (got ${what})`);
  };
  const flat = {
    null() { if (depth > 0) into.null(); },
    bool(b) { top("a boolean"); into.bool(b); },
    int(n) { top("an integer"); into.int(n); },
    number(x, single) { top("a float"); into.number(x, single); },
    char(c) { top("a char"); into.char(c); },
    string(s) { top("a string"); into.string(s); },
    variant(name) {
      if (depth > 0) return into.variant(name);
      into.key(name);
      into.null();
    },
    beginArray() { top("a sequence"); depth++; into.beginArray(); },
    beginTuple() { top("a tuple"); depth++; into.beginTuple(); },
    beginTupleStruct() { top("a tuple struct"); depth++; into.beginTupleStruct(); },
    element() { into.element(); },
    endArray() { depth--; into.endArray(); },
    beginObject() { if (depth++ > 0) into.beginObject(); },
    key(k) { into.key(k); },
    endObject() { if (--depth > 0) into.endObject(); },
    flat(value, write) { write(value, $jsonFlat(this)); },
  };
  return flat;
}
function $toJson(value, write, pretty) {
  const json = $jsonWriter(pretty);
  try {
    write(value, json);
  } catch (e) {
    if (e instanceof $JsonError) return { TAG: "Err", _0: { message: e.message, line: e.line, column: e.column } };
    throw e;
  }
  return { TAG: "Ok", _0: json.text };
}
// An `f32` as serde_json writes one: its shortest digits, zmij's, fixed from
// 1e-6 to 1e12, else `1.5e+16` (ADR 0122).
function $jsonF32(x) {
  if (!Number.isFinite(x)) return "null";
  if (x === 0) return Object.is(x, -0) ? "-0.0" : "0.0";
  const [digits, point] = $f32Digits(Math.abs(x));
  const sign = x < 0 ? "-" : "";
  const e = point - 1;
  if (e < -6 || e > 12) {
    const mantissa = digits.length > 1 ? `${digits[0]}.${digits.slice(1)}` : digits;
    return `${sign}${mantissa}e${e < 0 ? "-" : "+"}${Math.abs(e)}`;
  }
  if (point <= 0) return `${sign}0.${"0".repeat(-point)}${digits}`;
  if (point >= digits.length) return `${sign}${digits}${"0".repeat(point - digits.length)}.0`;
  return `${sign}${digits.slice(0, point)}.${digits.slice(point)}`;
}
