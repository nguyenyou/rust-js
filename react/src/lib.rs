//! [React](https://react.dev) for rust-js (ADR 0041). A component is a function
//! that returns an [`Element`], and elements are built with typed methods,
//! which rust-js prints as the JSX you'd write by hand (ADR 0040):
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
//!     <button onClick={() => setCount((count) => count + 1)}>+</button>
//!     {count}
//!   </div>;
//! }
//! ```
//!
//! Every item here is a binding: rustc checks the types, and the bodies never
//! run. Generic ones use `#[rust_js::link_name]` (ADR 0039).

#![feature(register_tool)]
#![register_tool(rust_js)]
// The bodies are never compiled, so they don't use their parameters.
#![allow(unused_variables)]

use core::marker::PhantomData;
use core::ops::Deref;
use std::thread::LocalKey;

use web::JsObject;

/// A React element: what a component returns, and what goes in children.
/// Made by an element function like [`html::div`], by [`component`], or by
/// [`fragment`]; its attributes and children are set by the methods below,
/// in the expression that makes it.
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

macro_rules! nodes {
    ($($t:ty),*) => { $(impl Node for $t {})* };
}
nodes!(i8, i16, i32, u8, u16, u32, usize, f64);

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

/// What a text attribute takes: a string, or an `Option` of one, which leaves
/// the attribute out when `None`.
pub trait Text {}

impl Text for &str {}
impl Text for String {}
impl<T: Text + ?Sized> Text for &T {}
impl<T: Text> Text for Option<T> {}

/// What a [`key`](Element::key) can be: a string or a number.
pub trait Key {}

impl Key for &str {}
impl Key for String {}
impl Key for i32 {}
impl Key for u32 {}
impl Key for usize {}
impl<T: Key + ?Sized> Key for &T {}

impl Element {
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
    pub fn r#ref<E>(self, r: Ref<Option<&'static E>>) -> Element {
        unreachable!()
    }

    /// Any attribute, by its name in JSX, which must be a string literal:
    /// `.attr("aria-hidden", "true")`, `.attr("data-id", id)`.
    #[rust_js::link_name = "prop"]
    pub fn attr(self, name: &'static str, value: impl Text) -> Element {
        unreachable!()
    }
}

/// Attribute methods, one per React prop: `class_name` is `className`.
macro_rules! props {
    ($($(#[$doc:meta])* $name:ident: $ty:ty = $js:literal;)*) => {
        impl Element {
            $(
                $(#[$doc])*
                #[rust_js::link_name = concat!("prop ", $js)]
                pub fn $name(self, value: $ty) -> Element {
                    unreachable!()
                }
            )*
        }
    };
}

props! {
    id: impl Text = "id";
    class_name: impl Text = "className";
    title: impl Text = "title";
    lang: impl Text = "lang";
    dir: impl Text = "dir";
    role: impl Text = "role";
    hidden: bool = "hidden";
    tab_index: i32 = "tabIndex";
    draggable: bool = "draggable";
    spell_check: bool = "spellCheck";
    href: impl Text = "href";
    target: impl Text = "target";
    rel: impl Text = "rel";
    download: impl Text = "download";
    src: impl Text = "src";
    src_set: impl Text = "srcSet";
    sizes: impl Text = "sizes";
    alt: impl Text = "alt";
    width: impl Text = "width";
    height: impl Text = "height";
    loading: impl Text = "loading";
    /// `type`, a Rust keyword.
    r#type: impl Text = "type";
    name: impl Text = "name";
    value: impl Text = "value";
    default_value: impl Text = "defaultValue";
    placeholder: impl Text = "placeholder";
    checked: bool = "checked";
    default_checked: bool = "defaultChecked";
    disabled: bool = "disabled";
    read_only: bool = "readOnly";
    required: bool = "required";
    auto_focus: bool = "autoFocus";
    auto_complete: impl Text = "autoComplete";
    multiple: bool = "multiple";
    selected: bool = "selected";
    /// `for`, a Rust keyword, is `htmlFor` in React.
    html_for: impl Text = "htmlFor";
    min: impl Text = "min";
    max: impl Text = "max";
    step: impl Text = "step";
    pattern: impl Text = "pattern";
    accept: impl Text = "accept";
    max_length: u32 = "maxLength";
    min_length: u32 = "minLength";
    rows: u32 = "rows";
    cols: u32 = "cols";
    action: impl Text = "action";
    method: impl Text = "method";
    col_span: u32 = "colSpan";
    row_span: u32 = "rowSpan";
    date_time: impl Text = "dateTime";
    open: bool = "open";
    view_box: impl Text = "viewBox";
    fill: impl Text = "fill";
    stroke: impl Text = "stroke";
    stroke_width: impl Text = "strokeWidth";
    d: impl Text = "d";
    xmlns: impl Text = "xmlns";
}

/// Event handler methods: `on_click` is `onClick`. A handler is called with
/// React's event, and must not borrow anything, since it runs later: write
/// it `move |e| ..`.
macro_rules! events {
    ($($name:ident: $event:ty = $js:literal;)*) => {
        impl Element {
            $(
                #[rust_js::link_name = concat!("prop ", $js)]
                pub fn $name(self, handler: impl Fn(&$event) + 'static) -> Element {
                    unreachable!()
                }
            )*
        }
    };
}

events! {
    on_click: event::Mouse = "onClick";
    on_double_click: event::Mouse = "onDoubleClick";
    on_context_menu: event::Mouse = "onContextMenu";
    on_mouse_down: event::Mouse = "onMouseDown";
    on_mouse_up: event::Mouse = "onMouseUp";
    on_mouse_move: event::Mouse = "onMouseMove";
    on_mouse_enter: event::Mouse = "onMouseEnter";
    on_mouse_leave: event::Mouse = "onMouseLeave";
    on_pointer_down: event::Mouse = "onPointerDown";
    on_pointer_up: event::Mouse = "onPointerUp";
    on_pointer_move: event::Mouse = "onPointerMove";
    on_key_down: event::Keyboard = "onKeyDown";
    on_key_up: event::Keyboard = "onKeyUp";
    on_change: event::Change = "onChange";
    on_input: event::Change = "onInput";
    on_submit: event::Event = "onSubmit";
    on_focus: event::Event = "onFocus";
    on_blur: event::Event = "onBlur";
    on_scroll: event::Event = "onScroll";
}

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
/// one with no props. `M` only tells the two apart.
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

// ── Hooks ───────────────────────────────────────────────────────────────
//
// What a hook gives back is borrowed, `&'static T`: React keeps it, and it's
// read-only, as React's state is (a new value is what renders again). A
// shared reference is `Copy`, so every handler can `move` it in. In JS it's
// the value itself.

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

/// What [`use_state`] gives to set the state: React's `setCount`.
pub struct SetState<T>(PhantomData<JsObject>, PhantomData<T>);

impl<T> Clone for SetState<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SetState<T> {}

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

/// What [`use_reducer`] gives to send an action: React's `dispatch`.
pub struct Dispatch<A>(PhantomData<JsObject>, PhantomData<A>);

impl<A> Clone for Dispatch<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A> Copy for Dispatch<A> {}

impl<A> Dispatch<A> {
    /// `dispatch(action)`.
    #[rust_js::link_name = "this()"]
    pub fn dispatch(&self, action: A) {
        unreachable!()
    }
}

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

/// [`useRef`](https://react.dev/reference/react/useRef): a box that keeps its
/// value across renders, without rendering again when it changes. For a DOM
/// element, start it with `None` and give it to [`Element::ref`].
#[rust_js::link_name = "react#useRef"]
pub fn use_ref<T>(initial: T) -> Ref<T> {
    unreachable!()
}

/// What [`use_ref`] gives: `{ current }`.
pub struct Ref<T>(PhantomData<JsObject>, PhantomData<T>);

impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Ref<T> {}

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

/// [`useId`](https://react.dev/reference/react/useId): an id unique to this
/// component, the same on every render, for `id` and `html_for`.
#[rust_js::link_name = "react#useId"]
pub fn use_id() -> String {
    unreachable!()
}

// ── Context and memo ────────────────────────────────────────────────────
//
// JS makes a context or a memoized component once, at a module's top level.
// Rust keeps such a value in a `thread_local!`, which rust-js compiles to
// just that, a `const` of its module (ADR 0037):
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

/// [`useContext`](https://react.dev/reference/react/useContext): the value of
/// the nearest provider above, or the context's default.
#[rust_js::link_name = "react#useContext"]
pub fn use_context<T>(context: &'static LocalKey<Context<T>>) -> &'static T {
    unreachable!()
}

/// A context's provider's props: `component(&THEME, Provider { value: "dark",
/// children })` is `<THEME value="dark">{children}</THEME>`, as React 19
/// writes a provider.
pub struct Provider<T> {
    pub value: T,
    pub children: Element,
}

pub struct ProvidesContext;

impl<T> Component<Provider<T>, ProvidesContext> for &'static LocalKey<Context<T>> {}

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

// ── Elements ────────────────────────────────────────────────────────────

/// The DOM's elements: `div()` is `<div>`.
pub mod html {
    use super::Element;

    macro_rules! elements {
        ($($name:ident = $tag:literal),* $(,)?) => {
            unsafe extern "Rust" {
                $(
                    #[doc = concat!("`<", $tag, ">`")]
                    #[link_name = concat!("<", $tag, ">")]
                    pub safe fn $name() -> Element;
                )*
            }
        };
    }

    elements! {
        a = "a", abbr = "abbr", address = "address", article = "article", aside = "aside",
        audio = "audio", b = "b", blockquote = "blockquote", br = "br", button = "button",
        canvas = "canvas", caption = "caption", cite = "cite", code = "code", col = "col",
        colgroup = "colgroup", dd = "dd", del = "del", details = "details", dfn = "dfn",
        dialog = "dialog", div = "div", dl = "dl", dt = "dt", em = "em", fieldset = "fieldset",
        figcaption = "figcaption", figure = "figure", footer = "footer", form = "form",
        h1 = "h1", h2 = "h2", h3 = "h3", h4 = "h4", h5 = "h5", h6 = "h6", header = "header",
        hr = "hr", i = "i", iframe = "iframe", img = "img", input = "input", ins = "ins",
        kbd = "kbd", label = "label", legend = "legend", li = "li", main = "main", mark = "mark",
        menu = "menu", meter = "meter", nav = "nav", ol = "ol", optgroup = "optgroup",
        option = "option", output = "output", p = "p", picture = "picture", pre = "pre",
        progress = "progress", q = "q", s = "s", samp = "samp", section = "section",
        select = "select", small = "small", source = "source", span = "span", strong = "strong",
        sub = "sub", summary = "summary", sup = "sup", table = "table", tbody = "tbody",
        td = "td", textarea = "textarea", tfoot = "tfoot", th = "th", thead = "thead",
        time = "time", tr = "tr", u = "u", ul = "ul", var = "var", video = "video",
        // SVG
        svg = "svg", circle = "circle", ellipse = "ellipse", g = "g", line = "line",
        path = "path", polygon = "polygon", polyline = "polyline", rect = "rect",
        r#use = "use",
    }
}

// ── Events ──────────────────────────────────────────────────────────────

/// React's events, which wrap the DOM's. Each specific one derefs to
/// [`Event`](event::Event).
pub mod event {
    use super::*;

    /// A [React event](https://react.dev/reference/react-dom/components/common#react-event-object).
    pub struct Event(PhantomData<JsObject>);

    impl Event {
        /// Stop the browser's default action, like submitting a form.
        #[rust_js::link_name = "preventDefault"]
        pub fn prevent_default(&self) {
            unreachable!()
        }

        /// Stop parents' handlers seeing it.
        #[rust_js::link_name = "stopPropagation"]
        pub fn stop_propagation(&self) {
            unreachable!()
        }

        /// Where it happened.
        #[rust_js::link_name = "get target"]
        pub fn target(&self) -> &'static web::Element {
            unreachable!()
        }

        /// The element whose handler this is.
        #[rust_js::link_name = "get currentTarget"]
        pub fn current_target(&self) -> &'static web::Element {
            unreachable!()
        }

        /// The DOM's event that this wraps.
        #[rust_js::link_name = "get nativeEvent"]
        pub fn native_event(&self) -> &'static web::Event {
            unreachable!()
        }

        /// Its name, like `"click"`.
        #[rust_js::link_name = "get type"]
        pub fn type_(&self) -> String {
            unreachable!()
        }
    }

    macro_rules! events {
        ($($(#[$doc:meta])* $name:ident { $($(#[$fdoc:meta])* $method:ident: $ty:ty = $js:literal;)* })*) => {
            $(
                $(#[$doc])*
                pub struct $name(PhantomData<JsObject>);

                impl Deref for $name {
                    type Target = Event;

                    fn deref(&self) -> &Event {
                        // Never runs: rust-js compiles this `Deref` to the object itself.
                        unsafe { &*(self as *const Self as *const Event) }
                    }
                }

                impl $name {
                    $(
                        $(#[$fdoc])*
                        #[rust_js::link_name = concat!("get ", $js)]
                        pub fn $method(&self) -> $ty {
                            unreachable!()
                        }
                    )*
                }
            )*
        };
    }

    events! {
        /// A click, or another mouse or pointer event.
        Mouse {
            client_x: f64 = "clientX";
            client_y: f64 = "clientY";
            page_x: f64 = "pageX";
            page_y: f64 = "pageY";
            /// Which button: 0 is the main one.
            button: i32 = "button";
            alt_key: bool = "altKey";
            ctrl_key: bool = "ctrlKey";
            meta_key: bool = "metaKey";
            shift_key: bool = "shiftKey";
        }
        /// A key pressed or let go.
        Keyboard {
            /// What the key means, like `"Enter"` or `"a"`.
            key: String = "key";
            /// Which key it is on the keyboard, like `"KeyA"`.
            code: String = "code";
            repeat: bool = "repeat";
            alt_key: bool = "altKey";
            ctrl_key: bool = "ctrlKey";
            meta_key: bool = "metaKey";
            shift_key: bool = "shiftKey";
        }
        /// An `<input>`, `<select>` or `<textarea>` changing.
        Change {
            /// What's in it now: `e.target.value`.
            value: String = "target.value";
            /// Whether a checkbox is checked now: `e.target.checked`.
            checked: bool = "target.checked";
        }
    }
}

// ── React DOM ───────────────────────────────────────────────────────────

/// [`react-dom/client`](https://react.dev/reference/react-dom/client): putting
/// React on a page.
pub mod dom {
    use super::*;

    /// Where React renders, made by [`create_root`].
    pub struct Root(PhantomData<JsObject>);

    unsafe extern "Rust" {
        /// [`createRoot`](https://react.dev/reference/react-dom/client/createRoot).
        #[link_name = "react-dom/client#createRoot"]
        pub safe fn create_root(container: &web::Element) -> &'static Root;
    }

    impl Root {
        /// Show `children` in the root's element, replacing what was there.
        #[rust_js::link_name = "render"]
        pub fn render(&self, children: impl Node) {
            unreachable!()
        }

        /// Take React off the element.
        #[rust_js::link_name = "unmount"]
        pub fn unmount(&self) {
            unreachable!()
        }
    }
}
