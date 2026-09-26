//! Runtime support emitted only when a module uses it.

/// Runtime helpers, emitted into the module only when used.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Helper {
    TraitImpl,
    Index,
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
    StripPrefix,
    StripSuffix,
    SplitOnce,
    RsplitOnce,
    Try,
    Settle,
    UnwrapOk,
    Range,
    Cmp,
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
function $assertFailed(kind, left, right, message) {
  const op = kind === "Eq" ? "==" : kind === "Ne" ? "!=" : "matches";
  const why = message === undefined ? "" : ": " + message;
  throw new Error("assertion `left " + op + " right` failed" + why + "\n  left: " + $debug(left) + "\n right: " + $debug(right));
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
