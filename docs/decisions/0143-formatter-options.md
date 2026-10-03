# 0143. A `Formatter`'s options are an object its writers are given

Status: Accepted. Extends [0058](0058-format-options.md); supersedes the
`alternate` parameter of [0137](0137-pretty-debug.md).

## Context

A placeholder's options go to the `fmt` that shows the value, and Rust
hands them on with the `Formatter`:

```rust
impl fmt::Display for Meters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)?;   // the f64 is given `{:8.2}`'s width and precision
        f.write_str(" m")
    }
}
format!("[{:8.2}]", Meters(1.5))   // "[    1.50 m]"
```

A generic `T`'s `fmt`, a `dyn`'s, a derived `Debug`'s fields and a
builder's arguments are given them the same way. rust-js applied options
only where it could see the value at the placeholder (ADR 0058), and each
of these was an error. A writer's body can't know at compile time which
placeholder it's shown by: the same `metersDisplay_fmt` serves `{}`,
`{:8.2}` and `{:+}`.

ADR 0137 already gave writers one fact about their `Formatter`, whether
it's `{:#?}`, as a boolean parameter. The width, the precision, the sign
and the rest are more facts of the same kind.

## Decision

**A crate's writers take their `Formatter`'s options as one object, the
parameter after their value, which a placeholder makes and a writer hands
on:**

```js
function metersDisplay_fmt(meters, options) {
  let f = $formatFloat(meters[0], options, $displayF64);
  f += " m";
  return f;
}

metersDisplay_fmt([1.5], { width: 8, precision: 2 })
```

| Rust | JS |
|---|---|
| `{:8.2}` of a crate type | its `fmt` given `{ width: 8, precision: 2 }` |
| `{:#?}` | given `{ alternate: true }` |
| `{}` | given nothing: `options` is `undefined` |
| `self.0.fmt(f)`, a field, a builder's argument | given `options` |
| `f.alternate()` | `options?.alternate === true` |
| a number, a `bool` or a string shown given `options` | `$formatted(text, options)` |
| an `f64` or `f32` shown given `options` | `$formatFloat(x, options, $displayF64)` |

- **The object has only what the placeholder says:** `alternate`, `width`,
  `precision`, `fill`, `align`, `plus` and `zero`, each left out when it
  isn't set.
- **Options known at the placeholder still apply there** (ADR 0058):
  `{:5?}` of `Some(Meters(1.0))` pads nothing itself, and gives
  `{ width: 5 }` to `Meters`'s `fmt`; `{:5?}` of `Some(1)` is
  `Some(${"1".padStart(5)})`, with no object at all.
- **Run time applies them as compile time does.** `$formatted` is ADR
  0058's rules: a number takes a sign and zeros and pads on the right by
  default; a string or a `bool` is cut to its precision and pads on the
  left. A string's and a `char`'s `{:?}` ignore them, as std's do.
- **Dictionaries take the object too:** a generic `T`'s `fmt`
  (`TDisplay.fmt(value, { width: 6, align: "Right" })`), a `dyn`'s, and std's
  `Display` and `Debug` dictionaries, which apply it.
- **Only a crate that needs it pays for it.** Its writers take `options`
  if it shows anything with `{:#?}`, asks `f.alternate()`, or gives a
  placeholder's options to what isn't a number, a `bool`, a `char` or a
  string. Leaves inside writers apply `options` only in a crate that does
  the last: a crate whose only option is `{:#?}`'s has no width to apply.
- **Still errors, where options would be dropped:** a placeholder's options
  for a `&dyn Debug` made elsewhere, which is the string it shows already
  (ADR 0060), and for serde_json's types, whose `fmt`s are rust-js's own.

**A writer can ask its `Formatter` what it was given,** and `f.pad(s)`
applies it, as a `str`'s `Display` does:

| Rust | JS |
|---|---|
| `f.width()`, `f.precision()` | `options?.width`, `options?.precision`: `None` is `undefined` |
| `f.fill()` | `options?.fill ?? " "` |
| `f.align()` | `options?.align`: `"Left"`, `"Right"` or `"Center"`, `fmt::Alignment`'s variants |
| `f.sign_plus()`, `f.sign_aware_zero_pad()` | `options?.plus === true`, `options?.zero === true` |
| `f.pad(s)` | `$formatted(s, options)` |

- **A crate that asks takes options in its writers,** as one that gives
  them does, though no placeholder gives any: the answer is then `None`.
- **Still errors:** `f.sign_minus()` and `f.pad_integral(..)`.

## Why

- **It's how Rust works:** one `Formatter`, handed on. Each `fmt` gets the
  same options its Rust twin gets, so it shows the same text. The
  `options_handed_on` corpus case compares them with native Rust.
- **One object is simpler than a parameter per option.** Seven options
  would be seven parameters on each writer, and each new one would change
  every signature.
- **`undefined` for none keeps the plain call plain:** `pointDebug_fmt(p)`.

## Alternatives

- **A parameter per option.** Every writer would carry seven of them, most
  `undefined`.
- **A writer for each placeholder's options,** specialized at compile time.
  The same `fmt` would be copied for each `{:8.2}` and `{:+}`, and a
  generic `T`'s dictionary would need one of each.
- **Keeping 0137's boolean beside an object.** Two ways to say the same
  thing about one `Formatter`.

## Consequences

- `{:#?}`-only crates' writers take `options` where they took `alternate`,
  and are given `{ alternate: true }` where they were given `true`.
- A `&dyn Debug` field shown in a writer given a width shows its string
  unpadded: the width is known only at run time, after it was made.
- `f.width()`, `f.precision()`, `f.pad()` and the other ways to read a
  `Formatter` are still errors.
