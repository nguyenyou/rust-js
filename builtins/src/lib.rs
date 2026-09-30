//! The JS language for rust-js (ADR 0102): what JS has that Rust's `std`
//! doesn't, as ReScript's standard library has it. Its promises, errors and
//! regular expressions, its byte buffers, and its global functions. What the
//! browser adds is the webapi crate's; what `std` has, rust-js maps itself.
//!
//! It holds declarations only, so it's never compiled to JS: a program calls
//! what it declares, and the calls become plain JS.

#![feature(extern_types)]
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

/// A JS [`RegExp`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/RegExp),
/// for what Rust would use the `regex` crate for. String methods that take one
/// (`replace` with a closure, `matchAll`) are bindings a program declares,
/// typed for what it does with them.
pub struct RegExp(PhantomData<JsObject>);

pub mod reg_exp {
    use super::*;

    unsafe extern "Rust" {
        /// `new RegExp(pattern, flags)`: flags like `"gm"`.
        #[link_name = "new RegExp"]
        pub safe fn new(pattern: &str, flags: &str) -> &'static RegExp;

        #[link_name = "test"]
        pub safe fn test(this: &RegExp, text: &str) -> bool;
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
