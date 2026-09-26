//! Runtime support emitted only when a module uses it.

/// Runtime helpers, emitted into the module only when used.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Helper {
    TraitImpl,
    Index,
    At,
    DisplayF64,
    F64Max,
    F64Min,
    Div,
    Rem,
    Retain,
    Debug,
    Eq,
    AssertFailed,
    Unwrap,
    Some,
    SomeValue,
    SomeAt,
    Pop,
    Iterator,
    Insert,
    Add,
    Remove,
    OrInsert,
    OrInsertWith,
    SortedEntries,
    SortedKeys,
    StripPrefix,
    StripSuffix,
    SplitOnce,
    RsplitOnce,
    Try,
    Settle,
    UnwrapOk,
    Range,
    Cmp,
    PartialCmp,
    ToFixed,
    DebugF64,
    DebugChar,
    DebugFields,
    Plus,
    ZeroPad,
    Pad,
    CmpIn,
    CmpItems,
    ThenCmp,
    MaxBy,
    MinBy,
    Max,
    Min,
    Position,
}

impl Helper {
    pub fn source(self) -> &'static str {
        match self {
            Helper::Index => {
                "\nfunction $index(items, index) {\n  if (index < 0 || index >= items.length) throw new Error(`index out of bounds: the len is ${items.length} but the index is ${index}`);\n  return items[index];\n}\n"
            }
            // `v[i] = x`: `v[$at(v, i)] = x`, since JS would make the array longer.
            Helper::At => {
                "\nfunction $at(items, index) {\n  if (index < 0 || index >= items.length) throw new Error(`index out of bounds: the len is ${items.length} but the index is ${index}`);\n  return index;\n}\n"
            }
            Helper::DisplayF64 => include_str!("runtime/display_f64.js"),
            Helper::F64Max => {
                "\nfunction $f64Max(a, b) {\n  return Number.isNaN(a) ? b : Number.isNaN(b) ? a : Math.max(a, b);\n}\n"
            }
            Helper::F64Min => {
                "\nfunction $f64Min(a, b) {\n  return Number.isNaN(a) ? b : Number.isNaN(b) ? a : Math.min(a, b);\n}\n"
            }
            Helper::TraitImpl => {
                r#"
function $traitImpl(cache, keys, make) {
  for (let i = 0; i < keys.length - 1; i++) {
    const key = keys[i];
    if (!cache.has(key)) cache.set(key, new WeakMap());
    cache = cache.get(key);
  }
  const key = keys[keys.length - 1];
  if (!cache.has(key)) cache.set(key, make());
  return cache.get(key);
}
"#
            }
            Helper::Div => {
                r#"
function $div(a, b, min) {
  if (b === 0) {
    throw new Error("attempt to divide by zero");
  }
  if (a === min && b === -1) {
    throw new Error("attempt to divide with overflow");
  }
  return a / b;
}
"#
            }
            // `v.retain(keep)`: in place, so every reference to `v` sees it.
            Helper::Retain => {
                r#"
function $retain(v, keep) {
  let n = 0;
  for (const x of v) {
    if (keep(x)) {
      v[n++] = x;
    }
  }
  v.length = n;
}
"#
            }
            // `{:?}`: Rust's `Debug`, as far as the JS value shows it. Structs
            // print as `{ x: 1 }`: their type names aren't in the JS (ADR 0026).
            Helper::Debug => {
                r#"
function $debug(v) {
  if (typeof v === "string") {
    return JSON.stringify(v);
  }
  if (Array.isArray(v)) {
    return "[" + v.map($debug).join(", ") + "]";
  }
  if (v === undefined) {
    return "()";
  }
  // A `HashMap` and a `HashSet` (ADR 0059), in braces as Rust shows them.
  if (v instanceof Map) {
    return "{" + [...v].map(([k, x]) => $debug(k) + ": " + $debug(x)).join(", ") + "}";
  }
  if (v instanceof Set) {
    return "{" + [...v].map($debug).join(", ") + "}";
  }
  if (typeof v === "object" && v !== null) {
    return "{ " + Object.entries(v).map(([k, x]) => k + ": " + $debug(x)).join(", ") + " }";
  }
  return String(v);
}
"#
            }
            // `==` on structs, tuples, arrays and `Vec`s: a derived `PartialEq`
            // compares field by field, element by element.
            Helper::Eq => {
                r#"
function $eq(a, b) {
  if (a === b || (a == null && b == null)) {
    return true;
  }
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
    return false;
  }
  if (Array.isArray(a)) {
    return Array.isArray(b) && a.length === b.length && a.every((x, i) => $eq(x, b[i]));
  }
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => $eq(a[k], b[k]));
}
"#
            }
            // `assert_eq!` and `assert_ne!` failing, with Rust's message.
            Helper::Range => {
                r#"
function $range(start, end) {
  return Array.from({ length: Math.max(0, end - start) }, (_, i) => start + i);
}
"#
            }
            Helper::Cmp => {
                r#"
function $cmp(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}
"#
            }
            // `{:.2}` of an `f64`: its exact value, rounded to even on a tie, as
            // Rust does. JS's `toFixed` rounds a tie up, and past 1e21 it
            // switches to an exponent.
            Helper::ToFixed => {
                r#"
function $toFixed(value, digits) {
  if (Number.isNaN(value)) return "NaN";
  if (value === Infinity) return "inf";
  if (value === -Infinity) return "-inf";
  const sign = value < 0 || Object.is(value, -0) ? "-" : "";
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, Math.abs(value));
  const bits = view.getBigUint64(0);
  const exponent = Number((bits >> 52n) & 2047n);
  const fraction = bits & ((1n << 52n) - 1n);
  const significand = exponent === 0 ? fraction : fraction | (1n << 52n);
  const shift = exponent === 0 ? -1074 : exponent - 1075;
  // |value| * 10^digits, as a fraction.
  let numerator = significand * 10n ** BigInt(digits);
  let denominator = 1n;
  if (shift >= 0) numerator <<= BigInt(shift);
  else denominator <<= BigInt(-shift);
  let rounded = numerator / denominator;
  const twice = 2n * (numerator % denominator);
  if (twice > denominator || (twice === denominator && rounded % 2n === 1n)) rounded += 1n;
  const text = rounded.toString().padStart(digits + 1, "0");
  return sign + (digits === 0 ? text : text.slice(0, -digits) + "." + text.slice(-digits));
}
"#
            }
            // `{:?}` of an `f64`: `1.0`, and `1e16` or `1e-5` past `[1e-4, 1e16)`.
            Helper::DebugF64 => {
                r#"
function $debugF64(value) {
  const size = Math.abs(value);
  if (Number.isFinite(value) && size !== 0 && (size < 1e-4 || size >= 1e16)) {
    return value.toExponential().replace("e+", "e");
  }
  const text = $displayF64(value);
  return Number.isFinite(value) && !text.includes(".") ? text + ".0" : text;
}
"#
            }
            // `{:?}` of a `char`: in single quotes, escaped as Rust escapes it.
            Helper::DebugChar => {
                r#"
function $debugChar(c) {
  return "'" + (c === "'" ? "\\'" : c === '"' ? '"' : JSON.stringify(c).slice(1, -1)) + "'";
}
"#
            }
            // A derived `Debug` of a struct with more than five fields: its
            // fields' names, and their strings (ADR 0060).
            Helper::DebugFields => {
                r#"
function $debugFields(name, fields, values) {
  return name + " { " + fields.map((field, i) => field + ": " + values[i]).join(", ") + " }";
}
"#
            }
            // `{:+}`: a sign for a number that has none.
            Helper::Plus => {
                r#"
function $plus(text) {
  return text.startsWith("-") || text === "NaN" ? text : "+" + text;
}
"#
            }
            // `{:05}`: zeros after the sign and any `0x`.
            Helper::ZeroPad => {
                r#"
function $zeroPad(text, width) {
  const head = /^[+-]?(0[xbo])?/.exec(text)[0];
  return head + text.slice(head.length).padStart(width - head.length, "0");
}
"#
            }
            // `{:>8}` of a string: Rust counts its `char`s, where JS's `padStart`
            // would count UTF-16 units.
            Helper::Pad => {
                r#"
function $pad(text, width, align, fill = " ") {
  const room = width - [...text].length;
  if (room <= 0) return text;
  const before = align === ">" ? room : align === "^" ? Math.floor(room / 2) : 0;
  return fill.repeat(before) + text + fill.repeat(room - before);
}
"#
            }
            // `partial_cmp` of `f64`s: `None` if either is `NaN`.
            Helper::PartialCmp => {
                r#"
function $partialCmp(a, b) {
  return a < b ? -1 : a > b ? 1 : a === b ? 0 : undefined;
}
"#
            }
            // A fieldless enum's variants, in the order they're declared.
            Helper::CmpIn => {
                r#"
function $cmpIn(names, a, b) {
  return $cmp(names.indexOf(a), names.indexOf(b));
}
"#
            }
            // Item by item, then the shorter first, as Rust orders sequences.
            Helper::CmpItems => {
                r#"
function $cmpItems(a, b, cmp) {
  for (let i = 0; i < a.length && i < b.length; i++) {
    const order = cmp(a[i], b[i]);
    if (order !== 0) {
      return order;
    }
  }
  return $cmp(a.length, b.length);
}
"#
            }
            // The first that isn't `Equal`, unordered (`undefined`) included.
            Helper::ThenCmp => {
                r#"
function $thenCmp(...orders) {
  for (const order of orders) {
    if (order !== 0) {
      return order;
    }
  }
  return 0;
}
"#
            }
            // An iterator's `max`: the last of the greatest, as Rust's is. A generic
            // one's `Some` may be a box (ADR 0051).
            Helper::MaxBy => {
                r#"
function $maxBy(items, cmp, boxed = false) {
  if (items.length === 0) {
    return undefined;
  }
  const max = items.reduce((max, x) => (cmp(x, max) >= 0 ? x : max));
  return boxed ? $some(max) : max;
}
"#
            }
            // And `min`: the first of the least.
            Helper::MinBy => {
                r#"
function $minBy(items, cmp, boxed = false) {
  if (items.length === 0) {
    return undefined;
  }
  const min = items.reduce((min, x) => (cmp(x, min) < 0 ? x : min));
  return boxed ? $some(min) : min;
}
"#
            }
            Helper::Max => {
                r#"
function $max(items) {
  return items.length === 0 ? undefined : items.reduce((max, x) => (x >= max ? x : max));
}
"#
            }
            Helper::Min => {
                r#"
function $min(items) {
  return items.length === 0 ? undefined : items.reduce((min, x) => (x < min ? x : min));
}
"#
            }
            Helper::Position => {
                r#"
function $position(items, found) {
  const i = items.findIndex(found);
  return i < 0 ? undefined : i;
}
"#
            }
            Helper::Try => {
                r#"
function $try(f) {
  try {
    return { TAG: "Ok", _0: f() };
  } catch (e) {
    return { TAG: "Err", _0: e };
  }
}
"#
            }
            Helper::Settle => {
                r#"
function $settle(promise) {
  return promise.then((value) => ({ TAG: "Ok", _0: value }), (e) => ({ TAG: "Err", _0: e }));
}
"#
            }
            Helper::UnwrapOk => {
                r#"
function $unwrapOk(result, message = "called `Result::unwrap()` on an `Err` value") {
  if (result.TAG === "Err") {
    throw new Error(message + ": " + $debug(result._0));
  }
  return result._0;
}
"#
            }
            Helper::SplitOnce => {
                r#"
function $splitOnce(s, separator) {
  const i = s.indexOf(separator);
  return i < 0 ? undefined : [s.slice(0, i), s.slice(i + separator.length)];
}
"#
            }
            Helper::RsplitOnce => {
                r#"
function $rsplitOnce(s, separator) {
  const i = s.lastIndexOf(separator);
  return i < 0 ? undefined : [s.slice(0, i), s.slice(i + separator.length)];
}
"#
            }
            Helper::StripPrefix => {
                r#"
function $stripPrefix(s, prefix) {
  return s.startsWith(prefix) ? s.slice(prefix.length) : undefined;
}
"#
            }
            Helper::StripSuffix => {
                r#"
function $stripSuffix(s, suffix) {
  return s.endsWith(suffix) ? s.slice(0, s.length - suffix.length) : undefined;
}
"#
            }
            Helper::Some => {
                r#"
// `Some(x)` of a generic `T` (ADR 0051): `x`, unless it looks like `None`,
// as `undefined`, `null` or such a box does. Then it's a box one deeper.
function $some(x) {
  if (x == null) return { $someNone: 0 };
  if (typeof x === "object" && "$someNone" in x) return { $someNone: x.$someNone + 1 };
  return x;
}
"#
            }
            Helper::SomeValue => {
                r#"
// What's in a `$some`: the box one level shallower.
function $someValue(x) {
  if (x != null && typeof x === "object" && "$someNone" in x) {
    return x.$someNone === 0 ? undefined : { $someNone: x.$someNone - 1 };
  }
  return x;
}
"#
            }
            Helper::SomeAt => {
                r#"
// `Some` of the item at `index`, or `None` when there's none there.
function $someAt(items, index) {
  return index >= 0 && index < items.length ? $some(items[index]) : undefined;
}
"#
            }
            Helper::Pop => {
                r#"
// `Vec::pop` of a generic `T`: `None` for an empty one, else `Some` of the last.
function $pop(items) {
  return items.length === 0 ? undefined : $some(items.pop());
}
"#
            }
            Helper::Iterator => {
                r#"
// A Rust iterator of the crate's own as a JS one: its `next` returns the
// item, or `None` at the end. JS's iterator helpers are lazy, like Rust's
// adapters, so an endless one is fine until something wants all of it. A
// generic `next` may box its `Some` (ADR 0051): `boxed` unboxes it.
function $iterator(iterator, next, boxed = false) {
  return Iterator.from({
    next() {
      const item = next(iterator);
      if (item == null) {
        return { done: true, value: undefined };
      }
      return { done: false, value: boxed ? $someValue(item) : item };
    }
  });
}
"#
            }
            // A map's `insert` for its value: the one it replaced, or `None`.
            Helper::Insert => {
                r#"
function $insert(map, key, value) {
  const old = map.get(key);
  map.set(key, value);
  return old;
}
"#
            }
            // A set's `insert` for its value: whether it wasn't there yet.
            Helper::Add => {
                r#"
function $add(set, item) {
  const added = !set.has(item);
  set.add(item);
  return added;
}
"#
            }
            // A map's `remove` for its value: the one it took out, or `None`.
            Helper::Remove => {
                r#"
function $remove(map, key) {
  const old = map.get(key);
  map.delete(key);
  return old;
}
"#
            }
            // A `BTreeMap`'s entries, in its keys' order.
            Helper::SortedEntries => {
                r#"
function $sortedEntries(map, cmp) {
  return Array.from(map).sort((a, b) => cmp(a[0], b[0]));
}
"#
            }
            // A `BTreeSet`'s items, in order.
            Helper::SortedKeys => {
                r#"
function $sortedKeys(set, cmp) {
  return Array.from(set).sort(cmp);
}
"#
            }
            // `m.entry(k).or_insert(v)`: the value there, put there first if need be.
            Helper::OrInsert => {
                r#"
function $orInsert(map, key, value) {
  if (!map.has(key)) {
    map.set(key, value);
  }
  return map.get(key);
}
"#
            }
            // `or_insert_with(f)`: `f` runs only if there's none there.
            Helper::OrInsertWith => {
                r#"
function $orInsertWith(map, key, make) {
  if (!map.has(key)) {
    map.set(key, make());
  }
  return map.get(key);
}
"#
            }
            Helper::Unwrap => {
                r#"
function $unwrap(value, message = "called `Option::unwrap()` on a `None` value") {
  if (value == null) {
    throw new Error(message);
  }
  return value;
}
"#
            }
            Helper::AssertFailed => {
                r#"
// `left` and `right` are their `{:?}` strings already (ADR 0060).
function $assertFailed(kind, left, right, message) {
  const op = kind === "Eq" ? "==" : kind === "Ne" ? "!=" : "matches";
  const why = message === undefined ? "" : ": " + message;
  throw new Error("assertion `left " + op + " right` failed" + why + "\n  left: " + left + "\n right: " + right);
}
"#
            }
            Helper::Rem => {
                r#"
function $rem(a, b, min) {
  if (b === 0) {
    throw new Error("attempt to calculate the remainder with a divisor of zero");
  }
  if (a === min && b === -1) {
    throw new Error("attempt to calculate the remainder with overflow");
  }
  return a % b;
}
"#
            }
        }
    }
}
