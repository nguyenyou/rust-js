# 0043. React's whole API, gated by the release that added it

Status: Accepted. Extends [0041](0041-react.md).

## Context

The `react` crate covered hooks, context and `memo`. The goal is React's and
React DOM's whole supported API, as of the latest release (19.3).

Projects don't all use the latest React. Take one on React 18.2 that calls
`use_effect_event`, which arrived in 19.2. The binding would type-check and
compile, then crash in the browser: `useEffectEvent is not a function`.
Bindings are declarations, so nothing checks them against the React the
project runs.

**rescript-react** binds React by hand, and each React release gets its own
PR, like "Bindings for React 19.2 APIs". There's one version of the bindings
per React it targets: `@rescript/react` 0.14 is for React 19. A project on
another React picks the release of the bindings made for it. Nothing
generates them from React's `.d.ts` files, which leave out what a binding
needs: which props are really optional, and what can be `null`.

## Decision

**One crate, gated by React release.** An item that a later React added
carries `#[cfg(react = "19.2")]`, which means "React 19.2 or later". A build
for React *X.Y* passes `--cfg react="…"` for every minor release up to *X.Y*.
So for 18.2, nothing from 19.x exists, and using it is a compile error that
quotes the gate:

```text
error[E0432]: unresolved import `react::use_effect_event`
note: found an item that was configured out
   --> react/src/lib.rs:498:8
496 | #[cfg(react = "19.2")]
    |       -------------- the item is gated behind the `19.2` feature
```

The version comes from the project:

- **Vite:** `vite-plugin-rust-js` reads it from the project's
  `react/package.json` and builds the crate for it, each version in its own
  `target/react/<version>/`. On a gated error it also says which React is
  installed.
- **The CLI:** `react/build.sh --react 18.2.0`. Without `--react`, you get
  the latest.
- **Out of range:** React older than 18.0 is refused. One newer than the
  crate knows gets the latest API, with a warning.

The same gates cover what isn't an export: an event (`onScrollEnd`, 19.0), an
attribute (`popover`, 19.0), and React 19's ways of doing things. Those are
the context itself as a provider, async actions, form action functions, ref
cleanups and `useDeferredValue`'s initial value. `provider(&THEME)`, the
`<THEME.Provider>` that works in every version, stays ungated.

**What each release has is read from the release, not written down.**
`react/generate.ts` (`bun run generate:react`):

1. installs the latest patch of every minor release since 18.0;
2. for each, reads the exports of `react`, `react-dom`, `react-dom/client`,
   and `react-dom/server` and `react-dom/static` for both Web and Node,
   from both builds, since `act` is in the development build only;
3. reads the events and attributes React DOM itself registers, from its
   development build's own tables (`registrationNameDependencies`,
   `possibleStandardNames`);
4. writes `react/versions.json`: the release that first has each export,
   event and attribute, and the one that removed it;
5. generates `react/src/elements.rs` from that and from W3C's specs: every
   attribute and event as a gated method, every HTML and SVG element, and
   every CSS property as a `Style` method.

The hooks, built-in components and React DOM's APIs are written by hand, in
`react/src/lib.rs` and `dom.rs`, and gated by hand. The tests keep both
honest, against `versions.json`:

- **Each release:** rustdoc lists what the crate has when built for it, and
  every binding it imports must be in that release's exports.
- **Not gated late:** an export a release has, and the crate binds, must exist
  in that release's crate too.
- **Coverage:** every export of the latest release is bound, or on a list of
  what's left out, with the reason. That's legacy APIs (class components,
  which rust-js can't write, `createElement`, `cloneElement`, `Children`,
  `createRef`, `isValidElement`), internals, `unstable_*`, and
  `useFormState`, `useActionState`'s old name.
- **Generated file:** `elements.rs` must be what the generator makes of
  `versions.json`.

A new React release is then routine. Run the generator: `elements.rs` gains
the new events and attributes, gated. The coverage test fails for each new
export until it's bound with its gate, and the per-release test fails if a
gate is wrong.

**Two small compiler features came with it** (ADR 0039):

- **Objects built by methods:** a `{}` or `{__html}` binding makes an object,
  and `prop` on an object adds a field. That covers
  `style={{ color: "red" }}`, and options like
  `createRoot(el, { identifierPrefix })`, in the builder style elements
  already use. A key that isn't a JS name is quoted: `"--gap": "4px"`.
- **`#[rust_js::name = "hidden"]` on an enum variant:** its string in JS, for
  React's string unions, like `<Activity mode="hidden">`.

**Values that can't change where they're evaluated can be hoisted.** A handler
or a literal `style` can move out of the JSX even after code that runs,
because making a function or a literal evaluates nothing (ADR 0042).

## Why

- **The browser is the wrong place to learn a React version lacks an API.**
  rustc knows the version at compile time, so the error comes there,
  pointing at the gate.
- **One crate, not one per React version.** An API that React 19 added costs
  one attribute, not a new copy of the crate.
- **Facts from the releases, not from memory.** `act` being dev-only since
  19.1 and `popover` arriving in 19.0 aren't things anyone recalls reliably.
  The tests catch both a wrong gate and a missing binding.

## Alternatives

- **A crate per React version**, as rescript-react does (one release of the
  bindings per React). That means duplicated bindings, and the gates are
  implicit in which one you depend on.
- **Cargo features.** The crate isn't built by Cargo, and features are
  additive flags you'd have to keep in sync with the installed React by
  hand.
- **Checking at runtime** (`typeof useEffectEvent === "function"`). That's
  code React programs don't have, and it turns a compile error into a runtime
  branch.
- **Generating the hooks from `@types/react`.** The types are written by
  others, and lag and differ from React. The hand-written bindings are
  checked against the releases themselves instead.

## Consequences

- `react/versions.json` and `react/src/elements.rs` are generated and
  committed. Regenerating needs the network once, to install the releases
  into `target/react-versions/`.
- React DOM's option objects (`RootOptions`, `StreamOptions`, ..) aren't
  exports, so their fields' gates are checked only by hand, against React's
  docs.
- Not bound: class components and error boundaries (rust-js has no classes),
  and `<Suspense defer>`, which is experimental.
