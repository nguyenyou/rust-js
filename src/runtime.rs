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
    Repeat,
    OrInsert,
    OrInsertWith,
    SortedEntries,
    ToDigit,
    Lines,
    SplitBy,
    Pow,
    Powi,
    Round,
    Checked,
    CheckedDiv,
    RemEuclid,
    DivEuclid,
    TrailingZeros,
    CountOnes,
    BinarySearch,
    RemoveOpt,
    Iter,
    Next,
    Peek,
    NextIf,
    Rest,
    RestStr,
    UnwrapErr,
    DebugParseError,
    SiftUp,
    SiftDown,
    HeapPush,
    HeapPop,
    HeapSorted,
    HeapFrom,
    ParseInt,
    ParseF64,
    ParseBool,
    ParseChar,
    SliceRange,
    Extend,
    InsertAt,
    RemoveAt,
    Swap,
    Truncate,
    Dedup,
    Windows,
    Chunks,
    Zip,
    TakeWhile,
    SkipWhile,
    Partition,
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
    DebugStr,
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
            // `{:?}` of a string, or of a `char` in `'`: quoted, with what Rust
            // doesn't print as it is escaped: controls, formats, private use,
            // separators, combining marks, and spaces other than `" "`.
            Helper::DebugStr => {
                r#"
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
            Helper::UnwrapErr => {
                r#"
function $unwrapErr(result, message = "called `Result::unwrap_err()` on an `Ok` value") {
  if (result.TAG === "Ok") {
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
            // `v.extend(items)` (ADR 0062): a `push` of each, not of an array
            // too long for a call's arguments.
            Helper::Extend => {
                r#"
function $extend(v, items) {
  for (const item of items) {
    v.push(item);
  }
}
"#
            }
            // `v.insert(i, x)`, which panics past the end, where `splice` wouldn't.
            Helper::InsertAt => {
                r#"
function $insertAt(v, index, item) {
  if (index > v.length) {
    throw new Error(`insertion index (is ${index}) should be <= len (is ${v.length})`);
  }
  v.splice(index, 0, item);
}
"#
            }
            // `v.remove(i)`: the item, and a panic past the end.
            Helper::RemoveAt => {
                r#"
function $removeAt(v, index) {
  if (index >= v.length) {
    throw new Error(`removal index (is ${index}) should be < len (is ${v.length})`);
  }
  return v.splice(index, 1)[0];
}
"#
            }
            Helper::Swap => {
                r#"
function $swap(v, a, b) {
  if (a >= v.length || b >= v.length) {
    throw new Error(`index out of bounds: the len is ${v.length} but the index is ${Math.max(a, b)}`);
  }
  [v[a], v[b]] = [v[b], v[a]];
}
"#
            }
            Helper::Truncate => {
                r#"
function $truncate(v, length) {
  if (length < v.length) {
    v.length = length;
  }
}
"#
            }
            // `v.dedup()`: each run of equal items as one.
            Helper::Dedup => {
                r#"
function $dedup(v) {
  let n = 0;
  for (const item of v) {
    if (n === 0 || v[n - 1] !== item) {
      v[n++] = item;
    }
  }
  v.length = n;
}
"#
            }
            // `v.windows(n)`: each run of `n` in a row. It panics for 0, as Rust's does.
            Helper::Windows => {
                r#"
function $windows(v, size) {
  if (size === 0) {
    throw new Error("window size must be non-zero");
  }
  return Array.from({ length: Math.max(0, v.length - size + 1) }, (_, i) => v.slice(i, i + size));
}
"#
            }
            Helper::Chunks => {
                r#"
function $chunks(v, size) {
  if (size === 0) {
    throw new Error("chunk size must be non-zero");
  }
  return Array.from({ length: Math.ceil(v.length / size) }, (_, i) => v.slice(i * size, i * size + size));
}
"#
            }
            // `a.zip(b)`: pairs, as many as the shorter has.
            Helper::Zip => {
                r#"
function $zip(a, b) {
  return Array.from({ length: Math.min(a.length, b.length) }, (_, i) => [a[i], b[i]]);
}
"#
            }
            Helper::TakeWhile => {
                r#"
function $takeWhile(items, keep) {
  const end = items.findIndex((item) => !keep(item));
  return end < 0 ? items.slice() : items.slice(0, end);
}
"#
            }
            Helper::SkipWhile => {
                r#"
function $skipWhile(items, skip) {
  const start = items.findIndex((item) => !skip(item));
  return start < 0 ? [] : items.slice(start);
}
"#
            }
            // `partition(p)`: those it holds for, and the rest.
            Helper::Partition => {
                r#"
function $partition(items, keep) {
  const yes = [];
  const no = [];
  for (const item of items) {
    (keep(item) ? yes : no).push(item);
  }
  return [yes, no];
}
"#
            }
            // `c.to_digit(radix)`: the digit, or `None`.
            Helper::ToDigit => {
                r#"
function $toDigit(c, radix) {
  const digit = parseInt(c, 36);
  return digit < radix ? digit : undefined;
}
"#
            }
            // `s.lines()`: without a last empty line, and each without its `\r`.
            Helper::Lines => {
                r#"
function $lines(s) {
  const lines = s.split("\n");
  const last = lines.pop();
  const ended = lines.map((line) => (line.endsWith("\r") ? line.slice(0, -1) : line));
  if (last !== "") {
    ended.push(last);
  }
  return ended;
}
"#
            }
            // `s.split(|c| ..)`: the pieces between the `char`s it's true of.
            // `x.pow(e)`: multiplied as `Math.imul` does, so what's past 2^32
            // wraps as Rust's does, where `x ** e` would lose the low bits.
            Helper::Pow => {
                r#"
function $pow(base, exp) {
  let result = 1;
  while (exp > 0) {
    if (exp & 1) {
      result = Math.imul(result, base);
    }
    base = Math.imul(base, base);
    exp >>>= 1;
  }
  return result;
}
"#
            }
            // `x.powi(n)`: the multiplications Rust's own `__powidf2` does, in
            // its order, so the result rounds the same.
            Helper::Powi => {
                r#"
function $powi(x, n) {
  const reciprocal = n < 0;
  let result = 1;
  while (true) {
    if (n & 1) {
      result *= x;
    }
    n = (n / 2) | 0;
    if (n === 0) {
      break;
    }
    x *= x;
  }
  return reciprocal ? 1 / result : result;
}
"#
            }
            // `x.round()`: a half away from zero, where `Math.round` goes up.
            Helper::Round => {
                r#"
function $round(x) {
  return Math.sign(x) * Math.round(Math.abs(x));
}
"#
            }
            // `a.checked_add(b)`: the exact result, or `None` out of range. An
            // integer is never -0, which `0 * -5` is in JS: `+ 0` makes it 0.
            Helper::Checked => {
                r#"
function $checked(value, lo, hi) {
  return value >= lo && value <= hi ? value + 0 : undefined;
}
"#
            }
            Helper::CheckedDiv => {
                r#"
function $checkedDiv(a, b, min) {
  return b === 0 || (a === min && b === -1) ? undefined : Math.trunc(a / b) + 0;
}
"#
            }
            // `a.rem_euclid(b)`: never negative. `$rem` panics as `%` does,
            // and `+ 0` makes its -0 (`-4 % 2`) the integer 0.
            Helper::RemEuclid => {
                r#"
function $remEuclid(a, b, min) {
  const r = $rem(a, b, min) + 0;
  return r < 0 ? (b < 0 ? r - b : r + b) : r;
}
"#
            }
            Helper::DivEuclid => {
                r#"
function $divEuclid(a, b, min) {
  const q = Math.trunc($div(a, b, min)) + 0;
  return a % b < 0 ? (b > 0 ? q - 1 : q + 1) : q;
}
"#
            }
            Helper::TrailingZeros => {
                r#"
function $trailingZeros(x, bits) {
  return x === 0 ? bits : 31 - Math.clz32(x & -x);
}
"#
            }
            Helper::CountOnes => {
                r#"
function $countOnes(x) {
  let ones = 0;
  x >>>= 0;
  while (x !== 0) {
    ones += x & 1;
    x >>>= 1;
  }
  return ones;
}
"#
            }
            // `v.binary_search(&x)`: Rust's search, step for step, so that
            // among equal items it finds the one Rust does.
            Helper::BinarySearch => {
                r#"
function $binarySearch(items, x) {
  let size = items.length;
  if (size === 0) {
    return { TAG: "Err", _0: 0 };
  }
  let base = 0;
  while (size > 1) {
    const half = size >>> 1;
    const mid = base + half;
    if (!(items[mid] > x)) {
      base = mid;
    }
    size -= half;
  }
  const found = items[base];
  return found === x ? { TAG: "Ok", _0: base } : { TAG: "Err", _0: base + (found < x ? 1 : 0) };
}
"#
            }
            // `{:?}` of a parse error, which is its message: its kind, by the message.
            Helper::DebugParseError => {
                r#"
function $debugParseError(message, name) {
  const kinds = {
    "cannot parse integer from empty string": "Empty",
    "invalid digit found in string": "InvalidDigit",
    "number too large to fit in target type": "PosOverflow",
    "number too small to fit in target type": "NegOverflow",
    "number would be zero for non-zero type": "Zero",
    "cannot parse float from empty string": "Empty",
    "invalid float literal": "Invalid",
    "cannot parse char from empty string": "EmptyString",
    "too many characters in string": "TooManyChars",
  };
  return name === "ParseBoolError" ? name : `${name} { kind: ${kinds[message]} }`;
}
"#
            }
            // An iterator that knows where it is (ADR 0071): a `Peekable`, or one
            // that `next()` steps through. It's a JS iterator too.
            Helper::Iter => {
                r#"
function $iter(items) {
  return {
    items,
    at: 0,
    next() {
      return this.at < this.items.length ? { value: this.items[this.at++], done: false } : { value: undefined, done: true };
    },
    [Symbol.iterator]() {
      return this;
    },
  };
}
"#
            }
            // `it.next()` of any JS iterator: its next item, or `undefined` at the end.
            Helper::Next => {
                r#"
function $next(it) {
  const step = it.next();
  return step.done ? undefined : step.value;
}
"#
            }
            Helper::Peek => {
                r#"
function $peek(it) {
  return it.items[it.at];
}
"#
            }
            // `it.next_if(f)`: the next item if `f` says so, and then past it.
            Helper::NextIf => {
                r#"
function $nextIf(it, f) {
  return it.at < it.items.length && f(it.items[it.at]) ? it.items[it.at++] : undefined;
}
"#
            }
            // What's left of one, as an array; it then has nothing left.
            Helper::Rest => {
                r#"
function $rest(it) {
  const rest = it.items.slice(it.at);
  it.at = it.items.length;
  return rest;
}
"#
            }
            // `chars.as_str()`: what's left, as a string, still there to step through.
            Helper::RestStr => {
                r#"
function $restStr(it) {
  return it.items.slice(it.at).join("");
}
"#
            }
            // `d.remove(i)` of a `VecDeque`: the item, or `None` past the end.
            Helper::RemoveOpt => {
                r#"
function $removeOpt(items, index) {
  return index < items.length ? items.splice(index, 1)[0] : undefined;
}
"#
            }
            // A `BinaryHeap` (ADR 0068), step for step as Rust's: `sift_up`,
            // `sift_down_range` and `sift_down_to_bottom` move a hole, and
            // compare as `<=` and `>=` of the items' `cmp`.
            Helper::SiftUp => {
                r#"
function $siftUp(heap, start, pos, cmp) {
  const item = heap[pos];
  while (pos > start) {
    const parent = (pos - 1) >>> 1;
    if (cmp(item, heap[parent]) <= 0) {
      break;
    }
    heap[pos] = heap[parent];
    pos = parent;
  }
  heap[pos] = item;
  return pos;
}
"#
            }
            Helper::SiftDown => {
                r#"
function $siftDown(heap, pos, end, cmp) {
  const item = heap[pos];
  let child = 2 * pos + 1;
  while (child <= end - 2) {
    if (cmp(heap[child], heap[child + 1]) <= 0) {
      child += 1;
    }
    if (cmp(item, heap[child]) >= 0) {
      heap[pos] = item;
      return pos;
    }
    heap[pos] = heap[child];
    pos = child;
    child = 2 * pos + 1;
  }
  if (child === end - 1 && cmp(item, heap[child]) < 0) {
    heap[pos] = heap[child];
    pos = child;
  }
  heap[pos] = item;
  return pos;
}
"#
            }
            Helper::HeapPush => {
                r#"
function $heapPush(heap, item, cmp) {
  heap.push(item);
  $siftUp(heap, 0, heap.length - 1, cmp);
}
"#
            }
            // The last item goes to the top, which then sinks to the bottom
            // and rises back: Rust's `sift_down_to_bottom`.
            Helper::HeapPop => {
                r#"
function $heapPop(heap, cmp) {
  if (heap.length === 0) {
    return undefined;
  }
  let top = heap.pop();
  if (heap.length > 0) {
    [top, heap[0]] = [heap[0], top];
    const end = heap.length;
    const item = heap[0];
    let pos = 0;
    let child = 1;
    while (child <= end - 2) {
      if (cmp(heap[child], heap[child + 1]) <= 0) {
        child += 1;
      }
      heap[pos] = heap[child];
      pos = child;
      child = 2 * pos + 1;
    }
    if (child === end - 1) {
      heap[pos] = heap[child];
      pos = child;
    }
    heap[pos] = item;
    $siftUp(heap, 0, pos, cmp);
  }
  return top;
}
"#
            }
            // `into_sorted_vec` and `BinaryHeap::from` take what they're given,
            // which may be a clone that was never made (ADR 0052): a copy, then.
            Helper::HeapSorted => {
                r#"
function $heapSorted(items, cmp) {
  const heap = items.slice();
  let end = heap.length;
  while (end > 1) {
    end -= 1;
    [heap[0], heap[end]] = [heap[end], heap[0]];
    $siftDown(heap, 0, end, cmp);
  }
  return heap;
}
"#
            }
            Helper::HeapFrom => {
                r#"
function $heapFrom(items, cmp) {
  const heap = items.slice();
  let n = heap.length >>> 1;
  while (n > 0) {
    n -= 1;
    $siftDown(heap, n, heap.length, cmp);
  }
  return heap;
}
"#
            }
            Helper::SplitBy => {
                r#"
function $splitBy(s, matches) {
  const pieces = [""];
  for (const c of s) {
    if (matches(c)) {
      pieces.push("");
    } else {
      pieces[pieces.length - 1] += c;
    }
  }
  return pieces;
}
"#
            }
            // `s.parse::<u32>()` and the other integers: a `Result`, whose `Err` is
            // what the error's `to_string()` would be.
            Helper::ParseInt => {
                r#"
function $parseInt(s, min, max) {
  const error = (message) => ({ TAG: "Err", _0: message });
  if (s === "") return error("cannot parse integer from empty string");
  if (!(min < 0 ? /^[+-]?[0-9]+$/ : /^\+?[0-9]+$/).test(s)) return error("invalid digit found in string");
  const n = Number(s);
  if (n > max) return error("number too large to fit in target type");
  if (n < min) return error("number too small to fit in target type");
  return { TAG: "Ok", _0: n };
}
"#
            }
            // `s.parse::<f64>()`: what Rust reads as a float, and no more (JS's
            // `Number` also takes `""`, `" 1"` and `"0x10"`).
            Helper::ParseF64 => {
                r#"
function $parseF64(s) {
  if (s === "") return { TAG: "Err", _0: "cannot parse float from empty string" };
  const lower = s.toLowerCase();
  const sign = lower.startsWith("-") ? -1 : 1;
  const rest = lower.replace(/^[+-]/, "");
  if (rest === "inf" || rest === "infinity") return { TAG: "Ok", _0: sign * Infinity };
  if (rest === "nan") return { TAG: "Ok", _0: NaN };
  if (!/^([0-9]+\.?[0-9]*|\.[0-9]+)(e[+-]?[0-9]+)?$/.test(rest)) return { TAG: "Err", _0: "invalid float literal" };
  return { TAG: "Ok", _0: sign * Number(rest) };
}
"#
            }
            Helper::ParseBool => {
                r#"
function $parseBool(s) {
  return s === "true" || s === "false"
    ? { TAG: "Ok", _0: s === "true" }
    : { TAG: "Err", _0: "provided string was not `true` or `false`" };
}
"#
            }
            Helper::ParseChar => {
                r#"
function $parseChar(s) {
  const chars = [...s];
  if (chars.length === 1) return { TAG: "Ok", _0: s };
  return { TAG: "Err", _0: chars.length === 0 ? "cannot parse char from empty string" : "too many characters in string" };
}
"#
            }
            // `&v[a..b]`: a copy, and Rust's panic out of bounds.
            Helper::SliceRange => {
                r#"
function $slice(items, start, end = items.length) {
  if (start > end) throw new Error(`slice index starts at ${start} but ends at ${end}`);
  if (end > items.length) throw new Error(`range end index ${end} out of range for slice of length ${items.length}`);
  return items.slice(start, end);
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
            // `vec![item; count]`: clone all but the last slot, which takes item.
            Helper::Repeat => {
                r#"
function $repeat(item, count, clone) {
  const result = [];
  if (count > 0) {
    for (let i = 1; i < count; i++) result.push(clone(item));
    result.push(item);
  }
  return result;
}
"#
            }
            // `m.entry(k).or_insert(v)`: initialize only a missing entry.
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
