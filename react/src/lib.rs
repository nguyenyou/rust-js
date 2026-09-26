//! [React](https://react.dev) and React DOM for rust-js (ADRs 0041, 0043). A
//! component is a function that returns an [`Element`], and elements are
//! built with typed methods, which rust-js prints as the JSX you'd write by
//! hand (ADR 0040):
//!
//! ```ignore
//! use react::html::{button, div};
//! use react::{Element, use_state};
//!
//! pub fn Counter() -> Element {
//!     let (count, set_count) = use_state(0);
//!     div().class_name("counter").children((
//!         button().on_click(move |_| set_count.update(|count| count + 1)).children("+"),
//!         count,
//!     ))
//! }
//! ```
//!
//! ```js
//! import { useState } from "react";
//!
//! export function Counter() {
//!   const [count, setCount] = useState(0);
//!   return <div className="counter">
//!     <button onClick={() => setCount((count) => count + 1 | 0)}>+</button>
//!     {count}
//!   </div>;
//! }
//! ```
//!
//! Every item here is a binding: rustc checks the types, and the bodies never
//! run. Generic ones use `#[rust_js::link_name]` (ADR 0039).
//!
//! # React versions
//!
//! This crate binds React's API as of the latest release, from 18.0 on. What
//! a later release added is gated by it: `#[cfg(react = "19.2")]` is "React
//! 19.2 or later". `react/build.sh --react <version>` builds the crate for the
//! version a project has installed, so using what that version lacks is a
//! compile error, not a crash in the browser (ADR 0043). `react/versions.json`
//! records which release first has each export, event and attribute, read
//! from the releases themselves.

#![feature(register_tool)]
#![register_tool(rust_js)]
// The bodies are never compiled, so they don't use their parameters.
#![allow(unused_variables)]

#[cfg(react = "19.0")]
use core::future::Future;
use core::marker::PhantomData;
use core::ops::Deref;
use std::thread::LocalKey;

use web::{JsError, JsObject, Promise};

pub mod dom;
mod elements;
pub mod event;

pub use elements::html;
/// The DOM, whose types React's APIs use: `react::web::FormData`.
pub use web;

/// A React element: what a component returns, and what goes in children.
/// Made by an element function like [`html::div`], by [`component`], by
/// [`fragment`], or by a built-in component like [`suspense`]; its
/// attributes and children are set by methods, in the expression that makes it.
pub struct Element(PhantomData<JsObject>);

/// What React renders as a child: elements, text and numbers, and tuples,
/// `Vec`s and `Option`s of them. A tuple is several children, `("Count is ",
/// count)`; a `Vec` is a list, whose items each need a [`key`](Element::key);
/// `None` is nothing. Each is the JS value React expects already, so nothing
/// converts them.
pub trait Node {}

impl Node for Element {}
impl Node for &str {}
impl Node for String {}
impl Node for () {}
impl<T: Node + ?Sized> Node for &T {}
impl<T: Node> Node for Option<T> {}
impl<T: Node> Node for Vec<T> {}

macro_rules! numbers {
    ($($t:ty),*) => { $(impl Node for $t {} impl Value for $t {})* };
}
numbers!(i8, i16, i32, u8, u16, u32, usize, f64);

macro_rules! tuples {
    ($($name:ident)+) => {
        impl<$($name: Node),+> Node for ($($name,)+) {}
        impl<$($name),+> Deps for ($($name,)+) {}
    };
}
tuples!(A);
tuples!(A B);
tuples!(A B C);
tuples!(A B C D);
tuples!(A B C D E);
tuples!(A B C D E F);
tuples!(A B C D E F G);
tuples!(A B C D E F G H);
tuples!(A B C D E F G H I);
tuples!(A B C D E F G H I J);
tuples!(A B C D E F G H I J K);
tuples!(A B C D E F G H I J K L);

/// What an attribute or style takes: text, a number (which React turns into
/// text, or pixels in a style), a `bool`, or an `Option` of one, which leaves
/// it out when `None`.
pub trait Value {}

impl Value for &str {}
impl Value for String {}
impl Value for bool {}
impl<T: Value + ?Sized> Value for &T {}
impl<T: Value> Value for Option<T> {}

/// What a [`key`](Element::key) can be: a string or a number.
pub trait Key {}

impl Key for &str {}
impl Key for String {}
impl Key for i32 {}
impl Key for u32 {}
impl Key for usize {}
impl<T: Key + ?Sized> Key for &T {}

/// A [`style`](Element::style) object, `{ color: "red", fontSize: 12 }`:
/// made by `Style::new()`, then CSS properties by name, `.color("red")`.
pub struct Style(PhantomData<JsObject>);

impl Style {
    #[rust_js::link_name = "{}"]
    pub fn new() -> Style {
        unreachable!()
    }

    /// Any property, like a custom one: `.set("--accent", "red")`. The name
    /// is a string literal.
    #[rust_js::link_name = "prop"]
    pub fn set(self, name: &'static str, value: impl Value) -> Style {
        unreachable!()
    }
}

/// `{ __html }`, for [`dangerously_set_inner_html`](Element::dangerously_set_inner_html).
pub struct InnerHtml(PhantomData<JsObject>);

#[rust_js::link_name = "{__html}"]
pub fn inner_html(html: impl Value) -> InnerHtml {
    unreachable!()
}

impl Element {
    /// Spread a props struct into this element. Fields keep the JS names of
    /// their Rust struct (including the crate's camel_case setting).
    #[rust_js::link_name = "prop ..."]
    pub fn props<P>(self, props: P) -> Element {
        unreachable!()
    }

    /// Its children: one [`Node`], or several as a tuple.
    #[rust_js::link_name = "prop children"]
    pub fn children(self, children: impl Node) -> Element {
        unreachable!()
    }

    /// Tells React which item of a list this is, across renders.
    #[rust_js::link_name = "prop key"]
    pub fn key(self, key: impl Key) -> Element {
        unreachable!()
    }

    /// Puts the DOM element in `r` once it's on the page (see [`use_ref`]).
    #[rust_js::link_name = "prop ref"]
    pub fn r#ref<E: 'static>(self, r: Ref<Option<&'static E>>) -> Element {
        unreachable!()
    }

    /// A ref callback: called with the DOM element once it's on the page.
    /// What it returns runs when the element leaves; before React 19,
    /// cleanups don't exist, and `f` is called with `None` instead.
    #[rust_js::link_name = "prop ref"]
    pub fn ref_callback<E: 'static, C: Cleanup>(self, f: impl Fn(Option<&'static E>) -> C + 'static) -> Element {
        unreachable!()
    }

    /// Any attribute, by its name in JSX, which must be a string literal:
    /// `.attr("aria-hidden", "true")`, `.attr("data-id", id)`.
    #[rust_js::link_name = "prop"]
    pub fn attr(self, name: &'static str, value: impl Value) -> Element {
        unreachable!()
    }

    /// [`style`](https://react.dev/reference/react-dom/components/common#applying-css-styles):
    /// `.style(Style::new().color("red"))` is `style={{ color: "red" }}`.
    #[rust_js::link_name = "prop style"]
    pub fn style(self, style: Style) -> Element {
        unreachable!()
    }

    /// `dangerouslySetInnerHTML={{ __html }}`: HTML that React doesn't escape.
    #[rust_js::link_name = "prop dangerouslySetInnerHTML"]
    pub fn dangerously_set_inner_html(self, html: InnerHtml) -> Element {
        unreachable!()
    }

    /// A `<form>`'s [`action`](https://react.dev/reference/react-dom/components/form)
    /// as a function: called with the form's data when it's submitted, in a
    /// Transition. A string URL is [`action`](Element::action).
    #[cfg(react = "19.0")]
    #[rust_js::link_name = "prop action"]
    pub fn action_fn<R: ActionResult<M>, M>(self, action: impl Fn(&'static web::FormData) -> R + 'static) -> Element {
        unreachable!()
    }

    /// A `<button>`'s or `<input>`'s `formAction` as a function, like [`action_fn`](Element::action_fn).
    #[cfg(react = "19.0")]
    #[rust_js::link_name = "prop formAction"]
    pub fn form_action_fn<R: ActionResult<M>, M>(self, action: impl Fn(&'static web::FormData) -> R + 'static) -> Element {
        unreachable!()
    }

    /// `action={formAction}`: [`use_action_state`]'s action, sent the form's data.
    #[cfg(react = "19.0")]
    #[rust_js::link_name = "prop action"]
    pub fn action_dispatch(self, dispatch: Dispatch<&'static web::FormData>) -> Element {
        unreachable!()
    }

    /// A stylesheet's or style's place among others, which React orders and
    /// hoists into `<head>`.
    #[cfg(react = "19.0")]
    #[rust_js::link_name = "prop precedence"]
    pub fn precedence(self, value: impl Value) -> Element {
        unreachable!()
    }
}

/// What an action returns: nothing, or, from React 19, a future, which
/// React awaits while its Transition is pending. `M` only tells them apart.
pub trait ActionResult<M> {}

pub struct SyncAction;
pub struct AsyncAction;

impl ActionResult<SyncAction> for () {}
#[cfg(react = "19.0")]
impl<F: Future<Output = ()>> ActionResult<AsyncAction> for F {}

// ── Hooks ───────────────────────────────────────────────────────────────
//
// What a hook gives back is borrowed, `&'static T`: React keeps it, and it's
// read-only, as React's state is (a new value is what renders again). A
// shared reference is `Copy`, so every handler can `move` it in. In JS it's
// the value itself. They're at the crate's root, not re-exported from a
// module, so using one a React version lacks says which version it needs.

/// Declares a JS function a hook gives back, `setCount` or `dispatch`: a
/// `Copy` handle, called with `this()` (ADR 0039).
macro_rules! handle {
    ($(#[doc = $doc:literal])* $name:ident<$t:ident>) => {
        $(#[doc = $doc])*
        pub struct $name<$t>(PhantomData<JsObject>, PhantomData<$t>);

        impl<$t> Clone for $name<$t> {
            fn clone(&self) -> Self {
                *self
            }
        }

        impl<$t> Copy for $name<$t> {}
    };
}

// ── State ───────────────────────────────────────────────────────────────

/// [`useState`](https://react.dev/reference/react/useState): the state, and
/// what sets it. `let (count, set_count) = use_state(0);` is
/// `const [count, setCount] = useState(0);`.
#[rust_js::link_name = "react#useState"]
pub fn use_state<T>(initial: T) -> (&'static T, SetState<T>) {
    unreachable!()
}

/// `useState(() => initial())`: the first value, computed on the first render only.
#[rust_js::link_name = "react#useState"]
pub fn use_state_with<T>(initial: impl Fn() -> T + 'static) -> (&'static T, SetState<T>) {
    unreachable!()
}

handle! {
    /// What [`use_state`] gives to set the state: React's `setCount`.
    SetState<T>
}

impl<T> SetState<T> {
    /// `setCount(value)`.
    #[rust_js::link_name = "this()"]
    pub fn set(&self, value: T) {
        unreachable!()
    }

    /// `setCount((count) => count + 1)`: a new state from the latest one,
    /// which it only reads.
    #[rust_js::link_name = "this()"]
    pub fn update(&self, f: impl Fn(&T) -> T + 'static) {
        unreachable!()
    }
}

/// [`useReducer`](https://react.dev/reference/react/useReducer): the state,
/// and what sends it actions, which `reducer` turns into the next state.
#[rust_js::link_name = "react#useReducer"]
pub fn use_reducer<S, A>(reducer: impl Fn(&S, A) -> S + 'static, initial: S) -> (&'static S, Dispatch<A>) {
    unreachable!()
}

/// `useReducer(reducer, arg, init)`: the first state is `init(arg)`, computed
/// on the first render only.
#[rust_js::link_name = "react#useReducer"]
pub fn use_reducer_with<S, A, I>(
    reducer: impl Fn(&S, A) -> S + 'static,
    arg: I,
    init: impl Fn(I) -> S + 'static,
) -> (&'static S, Dispatch<A>) {
    unreachable!()
}

handle! {
    /// What sends an action: React's `dispatch`, from [`use_reducer`] and
    /// [`use_action_state`].
    Dispatch<A>
}

impl<A> Dispatch<A> {
    /// `dispatch(action)`.
    #[rust_js::link_name = "this()"]
    pub fn dispatch(&self, action: A) {
        unreachable!()
    }
}

/// [`useActionState`](https://react.dev/reference/react/useActionState): the
/// state, an action that runs `action` with the previous state and its
/// payload, and whether one is pending. Give the action to a form with
/// [`Element::action_dispatch`], or call it in a Transition.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react#useActionState"]
pub fn use_action_state<S, P>(
    action: impl Fn(&S, P) -> S + 'static,
    initial: S,
) -> (&'static S, Dispatch<P>, bool) {
    unreachable!()
}

/// [`use_action_state`] with an `async` action: the state is what its
/// future gives, and it's pending until then.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react#useActionState"]
pub fn use_async_action_state<S, P, F: Future<Output = S>>(
    action: impl Fn(&S, P) -> F + 'static,
    initial: S,
) -> (&'static S, Dispatch<P>, bool) {
    unreachable!()
}

/// [`useOptimistic`](https://react.dev/reference/react/useOptimistic):
/// `value`, or what [`SetOptimistic`] set while an action is pending.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react#useOptimistic"]
pub fn use_optimistic<S>(value: S) -> (&'static S, SetOptimistic<S>) {
    unreachable!()
}

/// `useOptimistic(value, reducer)`: the optimistic state is `reducer`'s,
/// from the actions sent to it.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react#useOptimistic"]
pub fn use_optimistic_with<S, A>(value: S, reducer: impl Fn(&S, A) -> S + 'static) -> (&'static S, Dispatch<A>) {
    unreachable!()
}

handle! {
    /// What [`use_optimistic`] gives to set the optimistic state.
    SetOptimistic<S>
}

#[cfg(react = "19.0")]
impl<S> SetOptimistic<S> {
    #[rust_js::link_name = "this()"]
    pub fn set(&self, value: S) {
        unreachable!()
    }

    #[rust_js::link_name = "this()"]
    pub fn update(&self, f: impl Fn(&S) -> S + 'static) {
        unreachable!()
    }
}

// ── Context and refs ────────────────────────────────────────────────────

/// [`useContext`](https://react.dev/reference/react/useContext): the value of
/// the nearest provider above, or the context's default.
#[rust_js::link_name = "react#useContext"]
pub fn use_context<T>(context: &'static LocalKey<Context<T>>) -> &'static T {
    unreachable!()
}

/// [`useRef`](https://react.dev/reference/react/useRef): a box that keeps its
/// value across renders, without rendering again when it changes. For a DOM
/// element, start it with `None` and give it to [`Element::ref`].
#[rust_js::link_name = "react#useRef"]
pub fn use_ref<T>(initial: T) -> Ref<T> {
    unreachable!()
}

handle! {
    /// What [`use_ref`] gives: `{ current }`.
    Ref<T>
}

impl<T> Ref<T> {
    /// `ref.current`.
    #[rust_js::link_name = "get current"]
    pub fn current(&self) -> T {
        unreachable!()
    }

    /// `ref.current = value`.
    #[rust_js::link_name = "set current"]
    pub fn set_current(&self, value: T) {
        unreachable!()
    }
}

/// [`useImperativeHandle`](https://react.dev/reference/react/useImperativeHandle):
/// what a parent's ref to this component gets, made by `create`.
#[rust_js::link_name = "react#useImperativeHandle"]
pub fn use_imperative_handle<H>(r: Ref<Option<H>>, create: impl Fn() -> H + 'static, deps: impl Deps) {
    unreachable!()
}

// ── Effects ─────────────────────────────────────────────────────────────

/// What an effect's dependencies are: a tuple of values, `(count, name)` for
/// `[count, name]`, or `()` for `[]`, when it runs once.
pub trait Deps {}

impl Deps for () {}
impl<T, const N: usize> Deps for [T; N] {}

/// What an effect returns: nothing, or a function that cleans it up.
pub trait Cleanup {}

impl Cleanup for () {}
impl<F: FnOnce() + 'static> Cleanup for F {}

/// [`useEffect`](https://react.dev/reference/react/useEffect): run `effect`
/// after a render in which `deps` changed.
#[rust_js::link_name = "react#useEffect"]
pub fn use_effect<C: Cleanup>(effect: impl Fn() -> C + 'static, deps: impl Deps) {
    unreachable!()
}

/// `useEffect(effect)`: after every render.
#[rust_js::link_name = "react#useEffect"]
pub fn use_effect_on_every_render<C: Cleanup>(effect: impl Fn() -> C + 'static) {
    unreachable!()
}

/// [`useLayoutEffect`](https://react.dev/reference/react/useLayoutEffect):
/// [`use_effect`], before the browser paints.
#[rust_js::link_name = "react#useLayoutEffect"]
pub fn use_layout_effect<C: Cleanup>(effect: impl Fn() -> C + 'static, deps: impl Deps) {
    unreachable!()
}

/// `useLayoutEffect(effect)`: after every render, before the browser paints.
#[rust_js::link_name = "react#useLayoutEffect"]
pub fn use_layout_effect_on_every_render<C: Cleanup>(effect: impl Fn() -> C + 'static) {
    unreachable!()
}

/// [`useInsertionEffect`](https://react.dev/reference/react/useInsertionEffect):
/// before layout effects, for CSS-in-JS libraries to insert styles.
#[rust_js::link_name = "react#useInsertionEffect"]
pub fn use_insertion_effect<C: Cleanup>(effect: impl Fn() -> C + 'static, deps: impl Deps) {
    unreachable!()
}

/// `useInsertionEffect(effect)`: after every render.
#[rust_js::link_name = "react#useInsertionEffect"]
pub fn use_insertion_effect_on_every_render<C: Cleanup>(effect: impl Fn() -> C + 'static) {
    unreachable!()
}

/// [`useEffectEvent`](https://react.dev/reference/react/useEffectEvent): `f`,
/// which an effect can call and always see the latest props and state, without
/// being one of its dependencies. Call it only from effects.
#[cfg(react = "19.2")]
#[rust_js::link_name = "react#useEffectEvent"]
pub fn use_effect_event<F: 'static>(f: F) -> &'static F {
    unreachable!()
}

// ── Performance ─────────────────────────────────────────────────────────

/// [`useMemo`](https://react.dev/reference/react/useMemo): `f`'s value, computed
/// again only when `deps` change.
#[rust_js::link_name = "react#useMemo"]
pub fn use_memo<T>(f: impl Fn() -> T + 'static, deps: impl Deps) -> &'static T {
    unreachable!()
}

/// [`useCallback`](https://react.dev/reference/react/useCallback): the same
/// function across renders, until `deps` change.
#[rust_js::link_name = "react#useCallback"]
pub fn use_callback<F: 'static>(f: F, deps: impl Deps) -> &'static F {
    unreachable!()
}

/// [`useTransition`](https://react.dev/reference/react/useTransition): whether
/// a Transition is pending, and what starts one.
#[rust_js::link_name = "react#useTransition"]
pub fn use_transition() -> (bool, StartTransition) {
    unreachable!()
}

/// What [`use_transition`] gives: React's `startTransition`.
pub struct StartTransition(PhantomData<JsObject>);

impl Clone for StartTransition {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for StartTransition {}

impl StartTransition {
    /// `startTransition(action)`: the updates in `action` render without
    /// blocking the page. From React 19, `action` can be `async`.
    #[rust_js::link_name = "this()"]
    pub fn start<R: ActionResult<M>, M>(&self, action: impl FnOnce() -> R + 'static) {
        unreachable!()
    }
}

/// [`useDeferredValue`](https://react.dev/reference/react/useDeferredValue):
/// `value`, which lags behind while more urgent updates render.
#[rust_js::link_name = "react#useDeferredValue"]
pub fn use_deferred_value<T>(value: T) -> &'static T {
    unreachable!()
}

/// `useDeferredValue(value, initialValue)`: `initial` on the first render.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react#useDeferredValue"]
pub fn use_deferred_value_with<T>(value: T, initial: T) -> &'static T {
    unreachable!()
}

// ── Other ───────────────────────────────────────────────────────────────

/// [`useId`](https://react.dev/reference/react/useId): an id unique to this
/// component, the same on every render, for `id` and `html_for`.
#[rust_js::link_name = "react#useId"]
pub fn use_id() -> String {
    unreachable!()
}

/// What [`use_sync_external_store`]'s `subscribe` is given: call it when the
/// store changes.
pub struct Notify(PhantomData<JsObject>);

impl Clone for Notify {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Notify {}

impl Notify {
    #[rust_js::link_name = "this()"]
    pub fn call(&self) {
        unreachable!()
    }
}

/// [`useSyncExternalStore`](https://react.dev/reference/react/useSyncExternalStore):
/// a value from outside React. `subscribe` gets a [`Notify`] to call on each
/// change, and returns what unsubscribes; `snapshot` reads the value.
#[rust_js::link_name = "react#useSyncExternalStore"]
pub fn use_sync_external_store<T, U: Cleanup>(
    subscribe: impl Fn(Notify) -> U + 'static,
    snapshot: impl Fn() -> T + 'static,
) -> &'static T {
    unreachable!()
}

/// `useSyncExternalStore(subscribe, snapshot, serverSnapshot)`: on the server
/// and while hydrating, the value is `server_snapshot`'s.
#[rust_js::link_name = "react#useSyncExternalStore"]
pub fn use_sync_external_store_with_server<T, U: Cleanup>(
    subscribe: impl Fn(Notify) -> U + 'static,
    snapshot: impl Fn() -> T + 'static,
    server_snapshot: impl Fn() -> T + 'static,
) -> &'static T {
    unreachable!()
}

/// [`useDebugValue`](https://react.dev/reference/react/useDebugValue): a label
/// for a custom hook in React DevTools.
#[rust_js::link_name = "react#useDebugValue"]
pub fn use_debug_value<T>(value: T) {
    unreachable!()
}

/// `useDebugValue(value, format)`: formatted only when DevTools shows it.
#[rust_js::link_name = "react#useDebugValue"]
pub fn use_debug_value_with<T>(value: T, format: impl Fn(&T) -> String + 'static) {
    unreachable!()
}

// ── Components ──────────────────────────────────────────────────────────

/// An element of a component: `component(Counter, CounterProps { initial: 1 })`
/// is `<Counter initial={1} />`. Its props are a struct, whose `children`
/// field, if it has one, is the element's children; `()` for a component that
/// takes none, `component(App, ())`. A component's name starts with an
/// uppercase letter, as JSX and Fast Refresh need.
#[rust_js::link_name = "<*>"]
pub fn component<P, M>(component: impl Component<P, M>, props: P) -> Element {
    unreachable!()
}

/// What [`component`] takes: a function from its props to an [`Element`], or
/// one with no props, or one made by [`memo`], [`lazy`] or [`forward_ref`].
/// `M` only tells them apart.
pub trait Component<P, M> {}

pub struct NoProps;
pub struct WithProps;

impl<F: Fn() -> Element> Component<(), NoProps> for F {}
impl<P, F: Fn(P) -> Element> Component<P, WithProps> for F {}

/// Children with nothing around them: `<>..</>`.
#[rust_js::link_name = "<>"]
pub fn fragment(children: impl Node) -> Element {
    unreachable!()
}

/// [`<StrictMode>`](https://react.dev/reference/react/StrictMode).
#[rust_js::link_name = "<react#StrictMode>"]
pub fn strict_mode(children: impl Node) -> Element {
    unreachable!()
}

/// Declares a built-in component: `name()` makes its element, whose props
/// are set by its methods, and `.children(..)` finishes it.
macro_rules! built_in {
    ($(#[doc = $doc:literal])* $(#[cfg($cfg:meta)])? $name:ident = $make:ident $tag:literal { $($(#[doc = $pdoc:literal])* $prop:ident: $ty:ty = $js:literal;)* }) => {
        $(#[doc = $doc])*
        $(#[cfg($cfg)])?
        pub struct $name(PhantomData<JsObject>);

        #[doc = concat!("`<", $tag, ">`: set its props, then its children.")]
        $(#[cfg($cfg)])?
        #[rust_js::link_name = concat!("<react#", $tag, ">")]
        pub fn $make() -> $name {
            unreachable!()
        }

        $(#[cfg($cfg)])?
        impl Node for $name {}

        $(#[cfg($cfg)])?
        impl $name {
            $(
                $(#[doc = $pdoc])*
                #[rust_js::link_name = concat!("prop ", $js)]
                pub fn $prop(self, value: $ty) -> $name {
                    unreachable!()
                }
            )*

            /// Its children, which finish the element.
            #[rust_js::link_name = "prop children"]
            pub fn children(self, children: impl Node) -> Element {
                unreachable!()
            }
        }
    };
}

built_in! {
    /// [`<Fragment>`](https://react.dev/reference/react/Fragment) with a key,
    /// for a list item of several elements. Without one, [`fragment`] is `<>`.
    Fragment = keyed_fragment "Fragment" {
        key: impl Key = "key";
    }
}

built_in! {
    /// [`<Suspense>`](https://react.dev/reference/react/Suspense): shows
    /// `fallback` until its children stop suspending.
    Suspense = suspense "Suspense" {
        /// What to show while the children load.
        fallback: impl Node = "fallback";
    }
}

built_in! {
    /// [`<Profiler>`](https://react.dev/reference/react/Profiler): measures
    /// how long its children take to render, in development and profiling builds.
    Profiler = profiler "Profiler" {
        id: impl Value = "id";
        /// Called after each commit: `(id, phase, actual_duration,
        /// base_duration, start_time, commit_time)`, the times in milliseconds.
        on_render: impl Fn(&str, Phase, f64, f64, f64, f64) + 'static = "onRender";
    }
}

/// A [`Profiler`]'s render: the first, a later one, or one its effects caused.
pub enum Phase {
    #[rust_js::name = "mount"]
    Mount,
    #[rust_js::name = "update"]
    Update,
    #[rust_js::name = "nested-update"]
    NestedUpdate,
}

built_in! {
    /// [`<Activity>`](https://react.dev/reference/react/Activity): hides its
    /// children and keeps their state, or shows them.
    #[cfg(react = "19.2")]
    Activity = activity "Activity" {
        mode: ActivityMode = "mode";
    }
}

/// Whether an [`Activity`]'s children show.
#[cfg(react = "19.2")]
pub enum ActivityMode {
    #[rust_js::name = "visible"]
    Visible,
    #[rust_js::name = "hidden"]
    Hidden,
}

built_in! {
    /// [`<ViewTransition>`](https://react.dev/reference/react/ViewTransition):
    /// animates its children with the browser's View Transitions when they
    /// change in a Transition. A class prop is `"auto"`, `"none"`, or a CSS
    /// class for the `::view-transition-*` pseudo-elements.
    #[cfg(react = "19.3")]
    ViewTransition = view_transition "ViewTransition" {
        /// Its name, for a shared-element transition between two of them.
        name: impl Value = "name";
        enter: impl Value = "enter";
        exit: impl Value = "exit";
        update: impl Value = "update";
        share: impl Value = "share";
        default: impl Value = "default";
        on_enter: impl Fn(&ViewTransitionInstance, &Vec<String>) + 'static = "onEnter";
        on_exit: impl Fn(&ViewTransitionInstance, &Vec<String>) + 'static = "onExit";
        on_share: impl Fn(&ViewTransitionInstance, &Vec<String>) + 'static = "onShare";
        on_update: impl Fn(&ViewTransitionInstance, &Vec<String>) + 'static = "onUpdate";
    }
}

/// What a [`ViewTransition`]'s event gets: its pseudo-elements, to animate.
#[cfg(react = "19.3")]
pub struct ViewTransitionInstance(PhantomData<JsObject>);

#[cfg(react = "19.3")]
impl ViewTransitionInstance {
    #[rust_js::link_name = "get name"]
    pub fn name(&self) -> String {
        unreachable!()
    }

    /// `::view-transition-old`, `new`, `group` and `image-pair`.
    #[rust_js::link_name = "get old"]
    pub fn old(&self) -> &'static web::Element {
        unreachable!()
    }

    #[rust_js::link_name = "get new"]
    pub fn new(&self) -> &'static web::Element {
        unreachable!()
    }

    #[rust_js::link_name = "get group"]
    pub fn group(&self) -> &'static web::Element {
        unreachable!()
    }

    #[rust_js::link_name = "get imagePair"]
    pub fn image_pair(&self) -> &'static web::Element {
        unreachable!()
    }
}

// ── Context, memo, lazy ─────────────────────────────────────────────────
//
// JS makes a context, a memoized or lazy component once, at a module's top
// level. Rust keeps such a value in a `thread_local!`, which rust-js
// compiles to just that, a `const` of its module (ADR 0037):
//
//     thread_local! {
//         static THEME: Context<&'static str> = create_context("light");
//         static FAST_CARD: Memo<CardProps> = memo(Card);
//     }
//
//     const THEME = createContext("light");
//     const FAST_CARD = memo(Card);
//
// The rest take the key: `use_context(&THEME)`, `component(&FAST_CARD, props)`.

/// A [context](https://react.dev/reference/react/createContext): a value that
/// a component's descendants read with [`use_context`], from the nearest
/// provider above them, or `default` without one.
pub struct Context<T>(PhantomData<JsObject>, PhantomData<T>);

/// [`createContext`](https://react.dev/reference/react/createContext), in a `thread_local!`.
#[rust_js::link_name = "react#createContext"]
pub fn create_context<T>(default: T) -> Context<T> {
    unreachable!()
}

/// A context's provider's props: `component(&THEME, Provider { value: "dark",
/// children })` is `<THEME value="dark">{children}</THEME>`, as React 19
/// writes a provider. Before 19, [`provider`] gives `<THEME.Provider>`.
pub struct Provider<T> {
    pub value: T,
    pub children: Element,
}

pub struct ProvidesContext;

#[cfg(react = "19.0")]
impl<T> Component<Provider<T>, ProvidesContext> for &'static LocalKey<Context<T>> {}

/// `THEME.Provider`, a context's provider in every React version:
/// `component(provider(&THEME), Provider { value, children })`.
#[rust_js::link_name = "get Provider"]
pub fn provider<T>(this: &'static LocalKey<Context<T>>) -> ContextProvider<T> {
    unreachable!()
}

pub struct ContextProvider<T>(PhantomData<JsObject>, PhantomData<T>);

impl<T> Component<Provider<T>, ProvidesContext> for ContextProvider<T> {}

/// A component that [`memo`] made: React skips rendering it again while its
/// props are the same as last time.
pub struct Memo<P>(PhantomData<JsObject>, PhantomData<P>);

/// [`memo`](https://react.dev/reference/react/memo), in a `thread_local!`.
/// Props are the same when each field is (`Object.is`).
#[rust_js::link_name = "react#memo"]
pub fn memo<P, M>(component: impl Component<P, M>) -> Memo<P> {
    unreachable!()
}

/// `memo(component, arePropsEqual)`: the props are the same when `are_equal` says so.
#[rust_js::link_name = "react#memo"]
pub fn memo_with<P, M>(component: impl Component<P, M>, are_equal: impl Fn(&P, &P) -> bool + 'static) -> Memo<P> {
    unreachable!()
}

pub struct Memoized;

impl<P> Component<P, Memoized> for &'static LocalKey<Memo<P>> {}

/// A component [`lazy`] loads the first time it renders.
pub struct Lazy<P>(PhantomData<JsObject>, PhantomData<P>);

/// A JS module whose default export is a component taking `P`, as
/// [`import_module`] loads it.
pub struct Module<P>(PhantomData<JsObject>, PhantomData<P>);

/// [`lazy`](https://react.dev/reference/react/lazy), in a `thread_local!`:
/// `lazy(|| import_module("./Chart.jsx"))`. It suspends while it loads, so
/// render it inside a [`suspense`].
#[rust_js::link_name = "react#lazy"]
pub fn lazy<P>(load: impl Fn() -> Promise<Module<P>> + 'static) -> Lazy<P> {
    unreachable!()
}

/// `import(specifier)`: a module, loaded when it's first needed. The
/// specifier names the JS file, as the bundler sees it.
#[rust_js::link_name = "import"]
pub fn import_module<P>(specifier: &'static str) -> Promise<Module<P>> {
    unreachable!()
}

pub struct Loaded;

impl<P> Component<P, Loaded> for &'static LocalKey<Lazy<P>> {}

/// A component that [`forward_ref`] made: its parent's `ref` reaches it.
pub struct ForwardRef<P, H>(PhantomData<JsObject>, PhantomData<(P, H)>);

/// [`forwardRef`](https://react.dev/reference/react/forwardRef), in a
/// `thread_local!`: `render` gets the props and the parent's ref. From React
/// 19 a component can take `ref` as a prop instead.
#[rust_js::link_name = "react#forwardRef"]
pub fn forward_ref<P, H>(render: impl Fn(P, Ref<Option<H>>) -> Element + 'static) -> ForwardRef<P, H> {
    unreachable!()
}

pub struct Forwarded;

impl<P, H> Component<P, Forwarded> for &'static LocalKey<ForwardRef<P, H>> {}

// ── APIs ────────────────────────────────────────────────────────────────

/// [`startTransition`](https://react.dev/reference/react/startTransition):
/// the state updates in `action` render without blocking the page. From
/// React 19, `action` can be `async`.
#[rust_js::link_name = "react#startTransition"]
pub fn start_transition<R: ActionResult<M>, M>(action: impl FnOnce() -> R + 'static) {
    unreachable!()
}

/// [`addTransitionType`](https://react.dev/reference/react/addTransitionType):
/// names what the current Transition is, for [`ViewTransition`]'s classes.
#[cfg(react = "19.3")]
#[rust_js::link_name = "react#addTransitionType"]
pub fn add_transition_type(name: &str) {
    unreachable!()
}

/// [`act`](https://react.dev/reference/react/act): in a test, apply every
/// update `f` causes before it returns. Development builds only.
#[cfg(react = "18.3")]
#[rust_js::link_name = "react#act"]
pub fn act<R: ActionResult<M>, M>(f: impl FnOnce() -> R + 'static) -> Promise<()> {
    unreachable!()
}

/// [`cache`](https://react.dev/reference/react/cache): `f`, remembering its
/// results. For React Server Components only.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react#cache"]
pub fn cache<F: 'static>(f: F) -> F {
    unreachable!()
}

/// [`cacheSignal`](https://react.dev/reference/react/cacheSignal): aborted
/// when the render that [`cache`] belongs to is done. Server Components only;
/// `None` elsewhere.
#[cfg(react = "19.2")]
#[rust_js::link_name = "react#cacheSignal"]
pub fn cache_signal() -> Option<&'static web::AbortSignal> {
    unreachable!()
}

/// [`captureOwnerStack`](https://react.dev/reference/react/captureOwnerStack):
/// which components rendered the current one, in development only.
#[cfg(react = "19.1")]
#[rust_js::link_name = "react#captureOwnerStack"]
pub fn capture_owner_stack() -> Option<String> {
    unreachable!()
}

unsafe extern "Rust" {
    /// React's version, like `"19.3.0"`.
    #[link_name = "react#version"]
    pub safe static VERSION: &'static str;
}

/// What [`use_`] reads: a [`Promise`]'s value, a [`Context`]'s, or, for
/// [`dom::browser`], nothing.
#[cfg(react = "19.0")]
pub trait Usable {
    type Output;
}

#[cfg(react = "19.0")]
impl<T: 'static> Usable for &Promise<T> {
    type Output = &'static T;
}

#[cfg(react = "19.0")]
impl<T: 'static> Usable for &'static LocalKey<Context<T>> {
    type Output = &'static T;
}

/// [`use`](https://react.dev/reference/react/use): a promise's value,
/// suspending until it resolves, or a context's. Unlike a hook, it can be
/// called in a condition or a loop. `use` is a Rust keyword, hence the name.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react#use"]
pub fn use_<U: Usable>(value: U) -> U::Output {
    unreachable!()
}

/// What React gives a root's error callbacks with the error: where it happened.
pub struct ErrorInfo(PhantomData<JsObject>);

impl ErrorInfo {
    /// The components the error went through, as text.
    #[rust_js::link_name = "get componentStack"]
    pub fn component_stack(&self) -> Option<String> {
        unreachable!()
    }
}

/// What React's error callbacks get: whatever was thrown, usually an `Error`.
pub type Error = JsError;
