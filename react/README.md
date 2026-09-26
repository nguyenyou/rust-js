# react: React for rust-js

Write React components in Rust. rust-js compiles them to the components
you'd write in JSX by hand ([ADR 0041](../docs/decisions/0041-react.md)).
The crate binds all of React's and React DOM's API, as of React 19.3, for
whichever React from 18.0 on your project has installed
([ADR 0043](../docs/decisions/0043-react-versions.md)):

```rust
#![allow(non_snake_case)]

use react::html::button;
use react::{Element, use_state};

pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    button()
        .class_name("counter")
        .on_click(move |_| set_count.update(|count| count + 1))
        .children(("Count is ", count))
}
```

```jsx
// App.jsx
import { useState } from "react";

export function App() {
  const [count, setCount] = useState(0);
  return <button className="counter" onClick={() => setCount((count) => count + 1 | 0)}>Count is {count}</button>;
}
```

It's all bindings: rustc checks the types, and nothing of this crate ends up
in the JS.

| Rust | JSX |
|---|---|
| `div()`, `button()`, .. in `react::html` | `<div />`, `<button />` |
| `.class_name("a")`, `.html_for(id)`, `.r#type("button")` | `className="a"`, `htmlFor={id}`, `type="button"` |
| `.attr("aria-hidden", "true")` | `aria-hidden="true"` |
| `.style(Style::new().color("red").font_size(12))` | `style={{ color: "red", fontSize: 12 }}` |
| `.dangerously_set_inner_html(inner_html(html))` | `dangerouslySetInnerHTML={{ __html: html }}` |
| `.on_click(move \|e\| ..)` | `onClick={(e) => ..}` |
| `.children(("Count is ", count))` | `Count is {count}` |
| `.key(t.id)` | `key={t.id}` |
| `component(Card, CardProps { title, children })` | `<Card title={title}>{children}</Card>` |
| `component(App, ())` | `<App />` |
| `fragment((a, b))`, `keyed_fragment().key(k).children(..)` | `<>{a}{b}</>`, `<Fragment key={k}>..</Fragment>` |
| `suspense().fallback(spinner).children(..)` | `<Suspense fallback={spinner}>..</Suspense>` |
| `strict_mode(app)`, `profiler().id("a").on_render(f)`, `activity().mode(..)`, `view_transition()` | `<StrictMode>`, `<Profiler>`, `<Activity>`, `<ViewTransition>` |

Every attribute and event React DOM knows is a method, generated from React
DOM's own tables: `class_name`, `popover_target`, `on_pointer_down_capture`,
and so on. So is every HTML and SVG element, and every CSS property on
`Style`. Handlers get React's typed events, `event::Mouse`, `event::Keyboard`
and the rest, each dereferencing to the one it extends.

A component is a `pub fn` with a capitalized name that returns `Element`.
Its props, if it has any, are a struct, and a field named `children` holds
its children. A tuple of children is several of them, a `Vec` is a list
whose items need keys, and `None` renders nothing.

**Hooks** are React's, in snake case, with a `_with` variant for an optional
argument: `use_state`, `use_reducer`, `use_context`, `use_ref`,
`use_imperative_handle`, `use_effect`, `use_layout_effect`,
`use_insertion_effect`, `use_effect_event`, `use_memo`, `use_callback`,
`use_transition`, `use_deferred_value`, `use_id`, `use_sync_external_store`,
`use_debug_value`, `use_action_state`, `use_optimistic`, and React 19's
`use` as `use_`, since `use` is a Rust keyword.

- What a hook gives back is `&'static T`, read-only as React's state is, and
  `Copy`, so every handler can `move` it in.
- An effect's dependencies are a tuple: `(count, name)` is `[count, name]`,
  and `()` is `[]`.
- An effect's closure returns nothing, or the closure that cleans it up.

**Context and `memo`** are made once, as JS makes them at a module's top
level. In Rust that's a `thread_local!`, which rust-js compiles to just that:

```rust
thread_local! {
    static THEME: Context<&'static str> = create_context("light");
    static FAST_CARD: Memo<CardProps> = memo(Card);
}

pub fn Toolbar() -> Element {
    let theme = use_context(&THEME);
    component(&FAST_CARD, CardProps { title: theme })
}

pub fn App() -> Element {
    component(&THEME, Provider { value: "dark", children: component(Toolbar, ()) })
}
```

```jsx
const THEME = createContext("light");
const FAST_CARD = memo(Card);

export function Toolbar() {
  const theme = useContext(THEME);
  return <FAST_CARD title={theme} />;
}

export function App() {
  return <THEME value="dark">
    <Toolbar />
  </THEME>;
}
```

`memo_with(Card, |a, b| ..)` is `memo(Card, arePropsEqual)`.

Put a context in a module of its own, `mod theme;`, as React advises. Saving
a file runs its module again, and a context made there is a new one, so
React remounts everything under its provider and its state is lost. That's
the same in hand-written React. rust-js leaves an unchanged `theme.js` alone
when `App.rs` changes, so Fast Refresh keeps the state. `memo` needs nothing
like this.

**React DOM** is `react::dom`: `create_portal`, `flush_sync`, the resource
hints (`preload`, `preinit`, ..), `use_form_status`, `request_form_reset`
and `browser`. Under it:
- `dom::client`: `create_root` and `hydrate_root`, with `RootOptions`;
- `dom::server`: `render_to_string`, `render_to_readable_stream`,
  `render_to_pipeable_stream`, `resume`, ..;
- `dom::prerender`: `react-dom/static`'s `prerender` and the rest.

Options are objects built by methods:
`create_root_with(el, RootOptions::new().identifier_prefix("app-"))` is
`createRoot(el, { identifierPrefix: "app-" })`.

Left out on purpose:
- class components and error boundaries, since rust-js has no classes;
- React's other legacy APIs (`createElement`, `cloneElement`, `Children`,
  `createRef`, `isValidElement`);
- anything `unstable_`.

## React versions

What a React release after 18.0 added is gated by it, `#[cfg(react = "19.2")]`,
and the crate is built for the React your project has. On React 18.2,
`use_effect_event` doesn't compile, and the error says why:

```text
error[E0432]: unresolved import `react::use_effect_event`
note: found an item that was configured out
496 | #[cfg(react = "19.2")]
    |       -------------- the item is gated behind the `19.2` feature
```

[`versions.json`](versions.json) records which release first has each export,
event and attribute, read from the releases themselves by
[`generate.ts`](generate.ts). The tests check every binding against it, for
every release. When React has a new release, `bun run generate:react` reads
it. Then the tests say which of its exports still need binding, and with
which gate.

## Build it

```bash
react/build.sh -o target/libreact.rmeta                         # the latest React; also writes libweb.rmeta
react/build.sh -o target/react-18/libreact.rmeta --react 18.2.0 # for React 18.2
rust-js App.rs -- --extern react=target/libreact.rmeta -L target
```

`bun run build` does the first step. In a Vite project,
[vite-plugin-rust-js](../vite-plugin/index.js) does both, on every save, for
the React the project has installed. See
[examples/vite-react](../examples/vite-react/README.md).
