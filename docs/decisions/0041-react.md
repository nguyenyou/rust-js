# 0041. React: the `react` crate, and Vite with Fast Refresh

Status: Accepted. Uses ADRs 0038 (names), 0039 (generic bindings) and 0040
(JSX).

## Context

The goal is a React component written in Rust that compiles to the
component a React user would write. It should work in the official Vite
template (`create-vite --template react`: Vite 8, `@vitejs/plugin-react` 6,
React 19), with Fast Refresh keeping state on each save.

What Fast Refresh needs (plugin-react 6 on Vite 8 uses Oxc's refresh
transform):

- A `.jsx` file, or one importing `react/jsx-runtime`. Oxc parses JSX only
  in `.jsx`/`.tsx`.
- Components that are top-level functions with capitalized names, which is
  how Oxc finds them (`$RefreshReg$`).
- A module whose every export is a component, or an unchanged primitive
  constant. Otherwise the update goes up to the importer.
- Hooks called as `useX(..)`, whose order gives the component's signature.

**rescript-react** binds React with externals: `useState: (unit => 'state)
=> ('state, ('state => 'state) => unit)`. The setter takes only a function,
and there are `useEffect0`..`useEffect7` for each number of dependencies.
`@react.component` makes a props record, and names the function after the
file (`function Counter(props)`) so it shows up in DevTools and in Fast
Refresh.

## Decision

**The `react` crate** (`react/src/lib.rs`) binds React with ADR 0039's
generic bindings:

- **`Element`** is what a component returns. `html::div()` and the other DOM
  elements build one. Its attribute methods use React's names in snake case
  (`class_name`, `html_for`, `r#type`), with `attr(name, value)` for any
  other. Its event methods (`on_click`) take `move` closures of typed events
  (`event::Mouse`, `Keyboard`, `Change`).
- **`component(Card, CardProps { .. })`** is `<Card .. />`, and `fragment`
  and `strict_mode` are the rest.
- **Hooks:**
  - `use_state` / `use_state_with`
  - `use_reducer`
  - `use_effect`, `use_effect_on_every_render`, `use_layout_effect`
  - `use_memo`, `use_callback`
  - `use_ref`, with `Element::ref`
  - `use_id`
- **`dom::create_root(..).render(..)`** is React DOM's client.
- Traits say what goes where: `Node` for a child, `Text` for a text
  attribute, `Key`, `Deps`, and `Cleanup`, meaning an effect returns nothing
  or a function. None of them converts anything. Each value is already what
  React expects.

A component is a public function with a capitalized name
(`#![allow(non_snake_case)]`, as Dioxus and Leptos components are named),
returning `Element`:

```rust
pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    button()
        .class_name("counter")
        .on_click(move |_| set_count.update(|count| count + 1))
        .children(("Count is ", count))
}
```

```jsx
import { useState } from "react";

export function App() {
  const [count, setCount] = useState(0);
  return <button className="counter" onClick={() => setCount((count) => count + 1 | 0)}>Count is {count}</button>;
}
```

**What a hook gives back is `&'static T`.** React keeps the value, and it's
read-only, as React's state is: a new value is what renders again. A shared
reference is `Copy`, so every handler can `move` it in without cloning. In JS
it's the value itself. `SetState::update` takes `Fn(&T) -> T` for the same
reason, so an update copies before it changes anything. `set` and `update`
are both `setCount(..)`, since React's setter takes a value or a function.

**`vite-plugin-rust-js`** (`vite-plugin/index.js`) runs rust-js when Vite
starts and on every save of a `.rs` file. It writes `src/App.jsx` beside
`src/App.rs`, and from there it's an ordinary file of the project:

```
save App.rs ─► rust-js ─► App.jsx changes ─► Vite HMR ─► Fast Refresh keeps state
```

A compile error goes to Vite's overlay, and the page keeps the last JS that
compiled. `vite build` stops on one. Vite also picks up rust-js's source map,
so the browser shows `App.rs`.

**`examples/vite-react`** is `bun create vite --template react` (create-vite
9.2.1), with `App.jsx` rewritten as `App.rs`. The only other changes are
`rustJs()` in `vite.config.js`, a named import in `main.jsx`, and ignoring
the generated `App.jsx`.

## Why

- **The output is what React's docs teach**: `useState`, destructuring,
  JSX, and one exported function per component. So Fast Refresh, React
  DevTools and the Rules of Hooks see ordinary React.
- **Bindings, not a framework.** Nothing runs between Rust and React, and
  there's no runtime to ship.
- **`&'static T` is what React's rule looks like in Rust's types.** A
  handler that mutates state doesn't compile, where in JS it would mutate
  React's copy and not render.

## Alternatives

- **Setters that only take functions, and `use_effect0..7`**, as
  rescript-react has them. Rust has tuples of any size and methods, so one
  `use_effect(f, deps)` with `(a, b)` or `()` covers them all.
- **Owned state, `(T, SetState<T>)`.** Every handler would need a `clone()`,
  and nothing would stop in-place mutation.
- **A Rust framework on React** (signals, a virtual DOM of our own). That's
  another library, where this aims for no layer at all.

## Consequences

- The Rust names of props are its field names. A snake-case field is a
  snake-case prop, `initial_count={1}`.
- An `i32` state keeps Rust's arithmetic: `count + 1 | 0`, as ReScript has.
  An `f64` is plain `+`.
- Not yet: context, `memo`, `forwardRef`, `style` objects, portals and
  Suspense. The playground can't run React, since its Result frame has no
  module loader.
- The plugin compiles with the rust-js binary in this repository
  (`bun run build`). Publishing rust-js and the crates is a later step.
