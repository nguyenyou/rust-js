# JSX in Rust

`jsx!` accepts familiar tags, with Rust expressions inside braces. It is built
into rust-js: no import, procedural macro, or generated Rust file is needed.
The native compiler and the browser playground use the same parser.

```rust
use react::{Element, use_state};

pub fn Counter() -> Element {
    let (count, set_count) = use_state(0);
    jsx! {
        <button className="counter" onClick={move |_| set_count.update(|n| n + 1)}>
            {"Count is "}{count}
        </button>
    }
}
```

The compiler expands markup into the existing typed React bindings, then
rustc checks types and ownership. The result is ordinary JSX in a `.jsx`
module. Vite and React Fast Refresh keep their existing jobs; editing markup
preserves component state under React's usual refresh rules. Source maps
refer to the original Rust file, including individual handler statements.

## Elements and expressions

| Syntax | Meaning |
|---|---|
| `<div />`, `<svg>...</svg>` | HTML and SVG bindings |
| `<>...</>` | Fragment |
| `className="card"`, `disabled`, `aria-label="Close"` | Typed attributes, shorthand `true`, named attributes |
| `{value}` | Rust expression |
| `{"Hello "}` or `"Hello "` | Text, including its explicit whitespace |
| `{if show { Some(view) } else { None }}` | Conditional child |
| `{items}` | A `Vec` of children; put keys on its elements |
| `{/* comment */}` | Comment |
| `<Fragment key={id}>...</Fragment>` | Keyed fragment |

Use Rust for lists: `items.iter().map(|item| jsx! { <li key={item.id}>...</li> })`.
Use a Rust block for multiple statements: `title={let n = compute(); n.to_string()}`.
Bare prose and HTML entities are not parsed: write `{"A & B"}`, not `A &amp; B`.

## Components and props

```rust
use react::Element;

pub struct Props {
    pub title: &'static str,
    pub children: Element,
}

pub fn Card(props: Props) -> Element {
    jsx! { <section><h1>{props.title}</h1>{props.children}</section> }
}

pub fn App() -> Element {
    jsx! { <Card title="Welcome"><p>{"Hello"}</p></Card> }
}
```

A component is a capitalized, nongeneric function returning `Element`, with
zero parameters or one named struct parameter. The props type can have any
name. Missing, unknown, and wrongly typed props are compile errors. Modules
and aliases work: `<ui::Card />`, `<ui.Card />`, or `use ui::Card as Panel`.
Camel-case attribute names select snake-case Rust fields; the crate's existing
`#![rust_js::camel_case]` setting controls their emitted JavaScript names.
`children` must have the Rust type of the supplied child or tuple of children.

For an existing props value, use `<Card {...props} />`. To override its fields,
write `<Card {...Props { title: "New", ..props }} />`. A component accepts named
props or one spread, not both; this avoids giving Rust struct updates the
opposite precedence to JSX spreads. Children may follow a spread. DOM elements
also accept one final spread, such as `<div id="card" {...attrs} />`; its fields
use the struct's emitted JavaScript names and override earlier attributes.

`key` is separate from component props. Attributes, keys, spreads and children
are evaluated once in their written order. `Fragment`, `StrictMode`, `Suspense`
and `Activity` have direct syntax support. Other React APIs, generic components,
memo and context providers remain available through the existing builder and
`component(...)` APIs, including inside `{...}` expressions.

## Formatting

Run `bun run fmt` to format the Cargo workspace and the playground's Rust files.
It runs rustfmt for ordinary Rust, then uses the compiler's JSX parser to align
tags, props, nested JSX and expression blocks with four-space indentation.
`bun run fmt:check` checks without writing; the nightly workflow runs this check.

To format a particular file, including examples outside the Cargo workspace:

```bash
bun run fmt examples/vite-react/src/App.rs
```

The JSX pass changes leading whitespace only. It preserves existing line breaks,
literal contents, comments and other macros' bodies; it does not wrap long tags
or reformat expressions like rustfmt does. `#[rustfmt::skip]` on an enclosing
item or JSX invocation leaves it alone. This is a repository command, not an
editor format-on-save integration.

## Current boundaries

- Write `jsx!` directly in Rust expressions or statements. JSX inside another
  macro's token arguments, or emitted by another macro, is not expanded by this
  pass. Bind it first, then pass the value (`let item = jsx! { <p /> }; vec![item]`).
- This is a rust-js extension. Stock rustc and rust-analyzer do not expand it;
  editor completion inside the markup is not provided here. Use the formatting
  command above for indentation.
- The browser playground compiles and displays JSX. Its existing preview runner
  runs plain JavaScript/DOM programs; it does not mount React examples. Use the
  Vite example for interactive React development.
- Rust reports a missing component field at its generated props constructor,
  with the original JSX invocation shown as a second diagnostic label. Runtime
  source maps still point to the original Rust.

The [design record](decisions/0072-jsx-syntax.md) describes the compiler boundary.
