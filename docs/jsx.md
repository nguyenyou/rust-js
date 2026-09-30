# JSX in Rust

`jsx!` accepts familiar tags, with Rust expressions inside braces. It is built
into rust-js: no procedural macro or generated Rust file is needed. A file
with JSX imports it, `use react::jsx;`: to a plain rustc, an editor's or a
`cargo check`, react's `jsx!` is an `Element` it doesn't look inside, so the
rest of the crate type-checks there too (ADR 0113). The native compiler and
the browser playground use the same parser.

```rust
use react::{Element, jsx, use_state};

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

A component is a capitalized function returning `Element`, with
zero parameters or one named struct parameter. The props type can have any
name. Missing, unknown, and wrongly typed props are compile errors. Modules
and aliases work: `<ui::Card />`, `<ui.Card />`, or `use ui::Card as Panel`.
Camel-case attribute names select snake-case Rust fields; the crate's existing
`js::camel_case!();` setting controls their emitted JavaScript names.
`children` must have the Rust type of the supplied child or tuple of children.

For an existing props value, use `<Card {...props} />`. To override its fields,
write `<Card {...Props { title: "New", ..props }} />`. A component accepts named
props or one spread, not both; this avoids giving Rust struct updates the
opposite precedence to JSX spreads. Children may follow a spread. DOM elements
also accept one final spread, such as `<div id="card" {...attrs} />`; its fields
use the struct's emitted JavaScript names and override earlier attributes.

`key` is separate from component props. Attributes, keys, spreads and children
are evaluated once in their written order. `Fragment`, `StrictMode`, `Suspense`,
`Profiler`, `Activity` and `ViewTransition` have direct syntax support, gated
by the installed React version.

## One syntax for elements

Use `jsx!` for all element construction. The typed builders used by the compiler
are hidden from the public documentation, and handwritten calls to them are
rejected, including aliases and builder calls inside JSX expressions. Hooks,
styles, context creation and React DOM operations are ordinary Rust APIs.

Generic components infer type arguments from their props. When necessary, use
Rust's turbofish: `<Card::<i32> value={42} />`. Close it with `</Card>`.

Named `thread_local!` declarations of `Memo<Props>`, `Lazy<Props>`,
`ForwardRef<Props, Handle>` and `Context<T>` get the same props syntax as functions:

```rust
thread_local! {
    static FAST_CARD: Memo<CardProps> = memo(Card);
    static THEME: Context<&'static str> = create_context("light");
}

pub fn App() -> Element {
    jsx! {
        <THEME value="dark">
            <FAST_CARD title="Welcome" />
        </THEME>
    }
}
```

Use `<THEME.Provider ...>...</THEME.Provider>` for the provider syntax supported
on React 18 and later. `<THEME ...>` requires React 19. Module paths and imports
work for these declarations too. For a component value whose props type is not
available to the syntax pass (for example, an imported value or a local alias),
use `<Selected {...props} />`, with `{...()}` for no props.

DOM `ref` accepts a ref object or callback. `action` and `formAction` accept
URLs, or on React 19+, functions and action dispatches. Write
`style={Style::new().color("red")}` for a typed style object.

A `ForwardRef<Props, Handle>` tag accepts `ref={reference}` alongside its
named props; rustc checks the reference against `Handle`. For ordinary function
components, `ref` is a field named `r#ref` in the props struct, as in React 19.

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

## Syntax contract

`test/jsx.test.ts` checks complete generated modules against the committed
[`test/snapshots/jsx`](../test/snapshots/jsx) snapshots, including imports,
component names, expressions and formatting. These are the supported syntax
families; each new syntax feature must extend this suite before it is considered
supported.

| Snapshot | Covered forms |
|---|---|
| `grammar` | Self-closing and paired tags, empty fragment, quoted/raw/escaped text, numbers, booleans, unit, `Option`, tuples, lists, conditional/match/block children, comments, boolean shorthand, literal/expression/camel-case/dashed attributes, styles, raw HTML |
| `modules` | Components, imported aliases, fragments, keyed list elements, optional children, DOM/component spreads and Rust struct updates |
| `spreads-and-paths` | Final DOM spread precedence, spread with nested children, explicit `children`, dot and `::` paths, unit component values, keyed fragments |
| `component-kinds` | Inferred and explicit generics, nested type arguments and lifetimes, generic children, zero-prop functions, dynamic component values, memo, lazy, forwardRef, context and `.Provider` |
| `builtins` | Profiler, ViewTransition, Suspense, Activity, StrictMode, object/callback refs, function actions and URL form actions |
| `refs` | Forwarded refs, ordinary component ref props, handle typing and evaluation order |
| `react18` | Older provider/ref syntax and React-version boundaries |
| `nested-expressions` | Nested JSX props, contextual callbacks, Rust macros and literal contents |
| `evaluation-order` | Attribute/child/handler ordering and original source lines |
| `keys` | Reserved component keys, evaluation once, generated-name hygiene |
| `import-collisions` | Duplicate export names, local/parameter collisions, import aliases and source lines |
| `svg` | SVG tags and attribute capitalization |
| `many-children` | Forty siblings, beyond Rust's flat tuple implementations |
| `module-resolution` | Nested, inline and configured module paths |

Run `bun run test:snapshots` to check them. For an intentional output change,
run `bun run bless`, inspect the generated-code diff, then rerun the tests
without blessing. Runtime assertions and source-map checks run alongside the
snapshots; updating a snapshot cannot update those expectations automatically.
Invalid syntax, prop types, obsolete builders and unavailable React APIs must
fail without replacing previous output. Browser and Fast Refresh behavior have
separate Chromium tests in the full `bun test` suite.

This contract covers our Rust JSX grammar and documented boundaries, not every
possible Rust program or every combination of React behavior. Snapshots record
what we emit; independent behavior checks establish what that output does.
