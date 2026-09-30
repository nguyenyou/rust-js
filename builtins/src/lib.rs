//! The JS language for rust-js (ADR 0102): what JS has that Rust's `std`
//! doesn't, as ReScript's standard library has it. Its promises, errors and
//! regular expressions, its byte buffers, its JSON, and its global functions,
//! its timers too, as ReScript's has them: every JS runtime has them. What the
//! browser adds is the webapi crate's; what `std` has, rust-js maps itself.
//!
//! It holds declarations only, so it's never compiled to JS: a program calls
//! what it declares, and the calls become plain JS.

#![feature(extern_types)]
// `object::is` of an extern type, which is only `PointeeSized`.
#![feature(sized_hierarchy)]
// `#[rust_js::link_name]` on a generic function, `settle` (ADR 0039).
#![feature(register_tool)]
#![register_tool(rust_js)]

use core::marker::PhantomData;

unsafe extern "Rust" {
    /// Any JS object. Every type here and in webapi holds a `PhantomData` of
    /// it, which is how rust-js knows it's a JS object.
    pub type JsObject;
}

/// A JS [`Promise`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Promise)
/// of a `T`. `.await` on one is JS's `await`; a rejected one throws, like a
/// panic. See ADR 0029.
pub struct Promise<T>(PhantomData<JsObject>, PhantomData<T>);

impl<T> core::future::Future for Promise<T> {
    type Output = T;

    fn poll(self: core::pin::Pin<&mut Self>, _: &mut core::task::Context<'_>) -> core::task::Poll<T> {
        unreachable!("rust-js compiles `.await` to JS's `await`")
    }
}

/// `promise`, settled either way: its `.await` is `Ok` of what it fulfils
/// with, or `Err` of what it's rejected with, where the `.await` of the
/// promise itself would throw. For a promise of the webapi crate's, as
/// `settle(window::fetch(window, url)).await` is a network error's `Err`
/// (ADR 0035).
#[rust_js::link_name = "this"]
#[allow(unused_variables)]
pub fn settle<T>(this: Promise<T>) -> Promise<Result<T, &'static JsError>> {
    unreachable!()
}

unsafe extern "Rust" {
    /// Run a future without waiting for it, as from an event handler:
    /// `spawn(Box::new(async move { .. }))`. A JS promise is already
    /// running, so in JS this is the promise itself, left unawaited.
    #[link_name = "this"]
    pub safe fn spawn(this: Box<dyn core::future::Future<Output = ()>>);

    /// [`encodeURIComponent`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/encodeURIComponent):
    /// `text` for a part of a URL, a query's value say.
    #[link_name = "encodeURIComponent"]
    pub safe fn encode_uri_component(text: &str) -> String;

    /// [`encodeURI`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/encodeURI):
    /// `text` for a whole URL, its `/`, `?` and `#` kept.
    #[link_name = "encodeURI"]
    pub safe fn encode_uri(text: &str) -> String;

    /// [`decodeURIComponent`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/decodeURIComponent):
    /// what `encode_uri_component` made, or the `URIError` of what it can't have.
    #[link_name = "decodeURIComponent"]
    pub safe fn decode_uri_component(text: &str) -> Result<String, &'static JsError>;

    /// [`decodeURI`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/decodeURI):
    /// what `encode_uri` made, or the `URIError` of what it can't have.
    #[link_name = "decodeURI"]
    pub safe fn decode_uri(text: &str) -> Result<String, &'static JsError>;
}

/// What `set_timeout` gives, to clear it with: a number in a browser, an
/// object in Node.
pub struct TimeoutId(PhantomData<JsObject>);

/// What `set_interval` gives, to clear it with.
pub struct IntervalId(PhantomData<JsObject>);

// Timers: not the language's own, but every JS runtime's, browsers', workers'
// and Node's, as ReScript's standard library has them (ADR 0102).
unsafe extern "Rust" {
    /// [`setTimeout(callback, ms)`](https://developer.mozilla.org/docs/Web/API/Window/setTimeout):
    /// `callback` once, after at least `ms` milliseconds.
    #[link_name = "setTimeout"]
    pub safe fn set_timeout(callback: Box<dyn FnOnce()>, ms: u32) -> &'static TimeoutId;

    /// [`clearTimeout(id)`](https://developer.mozilla.org/docs/Web/API/Window/clearTimeout):
    /// the timeout's `callback` won't run, if it hasn't.
    #[link_name = "clearTimeout"]
    pub safe fn clear_timeout(id: &TimeoutId);

    /// [`setInterval(callback, ms)`](https://developer.mozilla.org/docs/Web/API/Window/setInterval):
    /// `callback` every `ms` milliseconds, until it's cleared.
    #[link_name = "setInterval"]
    pub safe fn set_interval(callback: Box<dyn FnMut()>, ms: u32) -> &'static IntervalId;

    /// [`clearInterval(id)`](https://developer.mozilla.org/docs/Web/API/Window/clearInterval):
    /// the interval's `callback` doesn't run again.
    #[link_name = "clearInterval"]
    pub safe fn clear_interval(id: &IntervalId);
}

/// A JS [`RegExp`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/RegExp),
/// for what Rust would use the `regex` crate for. `replace` with a string is
/// here; with a closure, and `matchAll`, are bindings a program declares, typed
/// for its pattern: a JS replacer is given the match, then each group, then
/// where it matched, so its Rust type is the pattern's.
pub struct RegExp(PhantomData<JsObject>);

pub mod reg_exp {
    use super::*;

    unsafe extern "Rust" {
        /// `new RegExp(pattern, flags)`: flags like `"gm"`.
        #[link_name = "new RegExp"]
        pub safe fn new(pattern: &str, flags: &str) -> &'static RegExp;

        #[link_name = "test"]
        pub safe fn test(this: &RegExp, text: &str) -> bool;

        /// [`text.replace(pattern, with)`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/String/replace):
        /// the first match, or with the `g` flag each, replaced with `with`,
        /// in which `$1` is the first group and `$&` the match.
        #[link_name = "replace"]
        pub safe fn replace(this: &str, pattern: &RegExp, with: &str) -> String;
    }
}

/// A JS number's methods, for where Rust's own `format!` isn't what's wanted.
pub mod number {
    unsafe extern "Rust" {
        /// [`x.toFixed(digits)`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Number/toFixed):
        /// `x` with `digits` decimals, as JS rounds, a tie away from zero:
        /// `2.5` is `"3"`, where `format!("{:.0}", 2.5)` is `"2"`, to even, as
        /// Rust rounds it. Between ties they agree. `-0` is `"0"`, `1e21` and up
        /// is `String(x)`, `"1e+21"`, and `Infinity` is `"Infinity"`, where
        /// `format!`'s is `"inf"`. `digits` is 0 to 100: more throws a `RangeError`.
        #[link_name = "toFixed"]
        pub safe fn to_fixed(this: f64, digits: u32) -> String;
    }
}

/// JS's [`JSON`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/JSON).
/// A Rust value's JSON is serde's (ADR 0077): `JSON.stringify` of one, as
/// rust-js has it in JS, isn't, since `None` is `undefined` and an `i64` a
/// `BigInt`, which it throws on. So it's typed for what it's exact for.
pub mod json {
    unsafe extern "Rust" {
        /// [`JSON.stringify(text)`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/JSON/stringify):
        /// `text`'s JSON, in quotes, with its `"`, `\` and control characters
        /// escaped, which is a JS string literal too.
        #[link_name = "JSON.stringify"]
        pub safe fn stringify(text: &str) -> String;
    }
}

/// Whatever a JS function threw, or a promise rejected with: usually an
/// [`Error`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Error).
/// An `extern` function that returns `Result<T, &JsError>` catches it (ADR 0035).
pub struct JsError(PhantomData<JsObject>);

pub mod js_error {
    use super::*;

    unsafe extern "Rust" {
        /// `String(e)`: an `Error`'s name and message, or any value as text.
        #[link_name = "String"]
        pub safe fn to_string(error: &JsError) -> String;

        /// `e instanceof Error`: what was thrown is an
        /// [`Error`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Error),
        /// which has a `message`. A promise may be rejected with anything.
        #[link_name = "instanceof Error"]
        pub safe fn is_error(this: &JsError) -> bool;

        /// [`e.message`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Error/message):
        /// an `Error`'s message, without its name, as `is_error(e)` says it is one.
        #[link_name = "get message"]
        pub safe fn message(this: &JsError) -> String;
    }
}

/// JS's [`Object`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Object)
/// functions, for a JS object an API takes or gives.
pub mod object {
    use super::*;

    /// [`Object.fromEntries(entries)`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Object/fromEntries):
    /// a JS object of these keys and values, as an API that takes a
    /// dictionary of them wants: `{ "aria-label": "Rust source" }`.
    #[rust_js::link_name = "Object.fromEntries"]
    #[allow(unused_variables)]
    pub fn from_entries<T>(entries: Vec<(String, T)>) -> &'static JsObject {
        unreachable!()
    }

    /// [`Object.is(a, b)`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Object/is):
    /// the same object, or the same value, where `NaN` is itself and `0`
    /// isn't `-0`. Of JS objects, whether they're one: `std::ptr::eq` isn't
    /// rust-js's.
    #[rust_js::link_name = "Object.is"]
    #[allow(unused_variables)]
    pub fn is<T: core::marker::PointeeSized>(a: &T, b: &T) -> bool {
        unreachable!()
    }
}

/// A JS [`ArrayBuffer`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/ArrayBuffer):
/// raw bytes, as `response::array_buffer` gives them.
pub struct ArrayBuffer(PhantomData<JsObject>);

pub mod array_buffer {
    use super::*;

    unsafe extern "Rust" {
        #[link_name = "get byteLength"]
        pub safe fn byte_length(this: &ArrayBuffer) -> u32;
    }
}

/// A JS [`Uint8Array`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array):
/// a view of the bytes in an `ArrayBuffer`, as `response::bytes` gives them.
pub struct Uint8Array(PhantomData<JsObject>);

pub mod uint8_array {
    use super::*;

    unsafe extern "Rust" {
        /// A view of all of `buffer`.
        #[link_name = "new Uint8Array"]
        pub safe fn new(buffer: &ArrayBuffer) -> &'static Uint8Array;

        /// How many bytes it views.
        #[link_name = "get length"]
        pub safe fn length(this: &Uint8Array) -> u32;

        /// The buffer it views.
        #[link_name = "get buffer"]
        pub safe fn buffer(this: &Uint8Array) -> &'static ArrayBuffer;
    }
}
