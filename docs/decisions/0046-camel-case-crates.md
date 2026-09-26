# 0046. `#![rust_js::camel_case]`: a crate's own names, the JS way

Status: Accepted. Extends [0038](0038-js-names-and-destructuring.md).

## Context

ADR 0038 renames variables and parameters to camelCase, `set_count` to
`setCount`, but not functions, fields or exports, because other JS code
uses those by name. In a React app, that leaves names a JS reader wouldn't
write, and one that React gets wrong:

- **Props are fields**, so `on_open` is a prop `on_open={..}`, where React
  code writes `onOpen`.
- **A custom hook is a function**, and React (its lint rules, React
  Compiler) finds hooks by their names: `useX`. A Rust `use_dark_mode`
  stays `use_dark_mode`, which React Compiler takes for an ordinary
  function, whose result it may cache. The playground's hook had to be
  written `useDarkMode` in Rust (ADR 0044).

Which names a crate's JS has is part of its API: the JS code that calls it
depends on them. So it's the crate's choice, and the same for every tool
that compiles it: the CLI, the Vite plugin, and the playground's
WebAssembly compiler.

## Decision

**`#![rust_js::camel_case]` in a crate's root makes its own functions and
fields camelCase in JS**, with ADR 0038's rule: underscores go and the next
letter is uppercase, leading and trailing underscores stay, and a name with
no lowercase letter (`MAX_SIZE`) stays.

| Rust | Without | With |
|---|---|---|
| `pub fn use_clicks()` | `use_clicks` | `useClicks` |
| `pub fn make_person(first_name: &str)` | `make_person(firstName)` | `makePerson(firstName)` |
| `Person { first_name: .. }` | `{ first_name: .. }` | `{ firstName: .. }` |
| `Shape::Rect { top_left, .. }` | `shape.top_left` | `shape.topLeft` |
| props `CardProps { on_press, .. }` | `on_press={..}` | `onPress={..}` |
| `pub fn App()`, `const MAX: u32` | the same | the same |

- **Only the crate's own names change.** Bindings keep JS's names, since
  those are JS's. The fields of another crate's structs (`react::Provider`)
  keep theirs, and so do variants' names.
- **`#[rust_js::name = ".."]` keeps one name as written**, on a function
  or a field, as it already names an enum variant (ADR 0039). That's for
  objects JS reads by a name that isn't camelCase, like the WASI shim's
  `{ wasi_snapshot_preview1 }`.
- **Two fields that are one property are an error.** `first_name` and
  `firstName` in one struct would overwrite each other:

  ```text
  error: rust-js: fields `first_name` and `firstName` are both `firstName` in JS
  ```

  Two functions that come to one name get a `$1`, as variables do (ADR 0038).

The playground uses it. The Vite example doesn't need it, since its names
have no underscores.

## Why

- **A crate attribute, not a compiler flag.** The names are the crate's API,
  so they're written with its code. Every tool that compiles the crate then
  gives the same names, and there's no option to keep in sync between the
  CLI, the Vite plugin and the playground's compiler.
- **Opt-in.** Without it, JS code calling a Rust library can read its Rust
  names and know the JS ones (ADR 0038's reason). A crate that is a React
  app, which no JS calls by name, is where camelCase fits.
- **One rule for every name.** Variables were already camelCase. The rest
  follow the same rule, so nothing is spelled two ways.

## Alternatives

- **Always camelCase** (ADR 0038 rejected it). That breaks JS that calls a
  library by its Rust names, and it changes every program's output.
- **A CLI flag.** The names would depend on how the crate was built, and the
  Vite plugin and the playground would each need the option passed on.
- **camelCase hooks only** (a `use_` function becomes `useX`). That fixes
  React's rule, but it leaves props snake_case, and it makes a naming rule
  out of a prefix.

## Consequences

- An object that JS reads by Rust's snake_case names, like an options
  struct for a binding, needs `#[rust_js::name]` on those fields in a
  `camel_case` crate.
- `{:?}` of a struct prints its JS keys, so a `camel_case` crate prints
  `firstName`.
- Names that come from another crate are that crate's choice: a
  `camel_case` crate can call one that isn't.
