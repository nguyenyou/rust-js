
// serde_json's `Value` (ADR 0083): `"Null"`, or `{ TAG, _0 }` of `Bool`,
// `Number`, `String`, `Array` or `Object`, whose `Map` is a JS `Map`. A
// `Number` is `{ kind, value }`: `"u"` and `"i"` are integers (a BigInt past
// 2^53), `"f"` a finite float.

// `Number::from` of an integer, and `Value::from` of an `f64`, which is
// `Null` unless it's finite.
function $jsonInt(n) {
  return { kind: n < 0 ? "i" : "u", value: n };
}
function $jsonFloat(x) {
  return Number.isFinite(x) ? { TAG: "Number", _0: { kind: "f", value: x } } : "Null";
}
// `Number::from_f64`: `None` unless it's finite.
function $jsonNumberOfF64(x) {
  return Number.isFinite(x) ? { kind: "f", value: x } : undefined;
}
// `Value::from` of an `Option`: `Null`, or what it holds, converted.
function $jsonOption(o, convert) {
  return o == null ? "Null" : convert(o);
}

function $jsonNumberWrite(n, json) {
  if (n.kind === "f") json.number(n.value);
  else json.int(n.value);
}

// serde_json's `Serialize` for `Value`: an object's keys in the order its
// `BTreeMap` keeps them.
function $jsonValueWrite(value, json) {
  if (value === "Null") return json.null();
  const { TAG: tag, _0: inner } = value;
  if (tag === "Bool") return json.bool(inner);
  if (tag === "Number") return $jsonNumberWrite(inner, json);
  if (tag === "String") return json.string(inner);
  if (tag === "Array") {
    json.beginArray();
    for (const item of inner) {
      json.element();
      $jsonValueWrite(item, json);
    }
    return json.endArray();
  }
  json.beginObject();
  for (const [key, item] of $sortedEntries(inner, $cmp)) {
    json.key(key);
    $jsonValueWrite(item, json);
  }
  json.endObject();
}

// `clone()`: a copy of each array and map, which change in place; a
// string or a number can't.
function $jsonValueClone(value) {
  if (value.TAG === "Array") return { TAG: "Array", _0: value._0.map($jsonValueClone) };
  if (value.TAG === "Object") return { TAG: "Object", _0: $jsonMapClone(value._0) };
  return value;
}
function $jsonMapClone(map) {
  return new Map(Array.from(map, ([key, item]) => [key, $jsonValueClone(item)]));
}

// `{}` of a `Value`, its JSON, and with `{:#}`, its pretty JSON.
function $jsonValueText(value, pretty) {
  return $toJson(value, $jsonValueWrite, pretty)._0;
}
function $jsonNumberText(n) {
  return n.kind === "f" ? $jsonNumber(n.value) : String(n.value);
}

// serde_json's `Debug`: `Null`, `Bool(true)`, `Number(1)`, `String("a")`,
// `Array [..]` and `Object {..}`, whose parts `{:#?}` puts on lines of their
// own, as a `Vec`'s and a map's.
function $debugJsonValue(value, alternate = false) {
  if (value === "Null") return "Null";
  const { TAG: tag, _0: inner } = value;
  if (tag === "Bool") return `Bool(${inner})`;
  if (tag === "Number") return $debugJsonNumber(inner);
  if (tag === "String") return `String(${$debugStr(inner)})`;
  if (tag === "Array") {
    const items = inner.map((item) => $debugJsonValue(item, alternate));
    return alternate ? "Array " + $pretty("[", items, "]") : `Array [${items.join(", ")}]`;
  }
  const entries = $sortedEntries(inner, $cmp).map(
    ([key, item]) => `${$debugStr(key)}: ${$debugJsonValue(item, alternate)}`,
  );
  return alternate ? "Object " + $pretty("{", entries, "}") : `Object {${entries.join(", ")}}`;
}
function $debugJsonNumber(n) {
  return `Number(${$jsonNumberText(n)})`;
}

// `as_bool`, `as_str`, `as_array`, `as_object`: what the variant `tag`
// holds, if it's that one.
function $jsonValueAs(value, tag) {
  return value.TAG === tag ? value._0 : undefined;
}
function $jsonValueF64(value) {
  return value.TAG === "Number" ? $jsonNumberF64(value._0) : undefined;
}
function $jsonNumberF64(n) {
  return Number(n.value);
}
// `as_u64()` and `as_i64()`: the integer, a BigInt, if it's of that kind
// and fits.
function $jsonNumberInt(n, kind) {
  const fits = kind === "u64" ? n.kind === "u" : $jsonNumberIsI64(n);
  return fits ? BigInt(n.value) : undefined;
}
function $jsonValueInt(value, kind) {
  return value.TAG === "Number" ? $jsonNumberInt(value._0, kind) : undefined;
}
function $jsonNumberIsI64(n) {
  return n.kind === "i" || (n.kind === "u" && n.value <= 9223372036854775807n);
}
// `is_f64`, `is_u64` and `is_i64` of a `Value`.
function $jsonValueIs(value, kind) {
  if (value.TAG !== "Number") return false;
  if (kind === "i64") return $jsonNumberIsI64(value._0);
  return value._0.kind === kind[0];
}

// `get(key)` of an object, or `get(i)` of an array, and `value[key]`, which is
// `Null` for what isn't there.
function $jsonGet(value, key) {
  if (typeof key === "string") return value.TAG === "Object" ? value._0.get(key) : undefined;
  return value.TAG === "Array" ? value._0[key] : undefined;
}
function $jsonIndex(value, key) {
  return $jsonGet(value, key) ?? "Null";
}

// serde_json's `PartialEq` of a `Value` and a `String`, a `bool` or a
// number: `as_str()`, `as_bool()`, `as_f64()`, `as_i64()` or `as_u64()`
// is it.
function $jsonValueEq(value, other, kind) {
  if (kind === "String" || kind === "Bool") return value.TAG === kind && value._0 === other;
  if (value.TAG !== "Number") return false;
  const n = value._0;
  if (kind === "f64") return Number(n.value) === other;
  if (kind === "f32") return (n.kind === "f" ? Math.fround(n.value) : $bigToF32(BigInt(n.value))) === other;
  if (kind === "i64") return $jsonNumberIsI64(n) && n.value == other;
  return n.kind === "u" && n.value == other;
}

// `serde_json::to_value`: a `Value` of what `write` writes, serde_json's
// value serializer. A `NaN` is `Null`, and a map's keys are strings.
function $toJsonValue(value, write) {
  const stack = [];
  let root;
  const put = (v) => {
    const top = stack[stack.length - 1];
    if (!top) root = v;
    else if (top.items) top.items.push(v);
    else top.entries.set(top.key, v);
  };
  const string = (s) => put({ TAG: "String", _0: s });
  const open = () => stack.push({ items: [] });
  const builder = {
    null: () => put("Null"),
    bool: (b) => put({ TAG: "Bool", _0: b }),
    int: (n) => put({ TAG: "Number", _0: $jsonInt(n) }),
    number: (x) => put($jsonFloat(x)),
    char: string,
    string,
    variant: string,
    beginArray: open,
    beginTuple: open,
    beginTupleStruct: open,
    element() {},
    endArray: () => put({ TAG: "Array", _0: stack.pop().items }),
    beginObject: () => stack.push({ entries: new Map() }),
    key(k) {
      stack[stack.length - 1].key = k;
    },
    endObject: () => put({ TAG: "Object", _0: stack.pop().entries }),
    flat(v, writeFlat) {
      writeFlat(v, $jsonFlat(builder));
    },
  };
  try {
    write(value, builder);
  } catch (e) {
    if (e instanceof $JsonError) return { TAG: "Err", _0: { message: e.message, line: e.line, column: e.column } };
    throw e;
  }
  return { TAG: "Ok", _0: root };
}
