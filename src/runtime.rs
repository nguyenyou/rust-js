//! Runtime support emitted only when a module uses it.

/// Runtime helpers, emitted into the module only when used.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Helper {
    Div,
    Rem,
    Retain,
    Debug,
    Eq,
    AssertFailed,
    Unwrap,
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

