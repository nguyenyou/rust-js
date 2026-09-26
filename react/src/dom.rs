//! [React DOM](https://react.dev/reference/react-dom): portals, resource
//! hints and forms here; putting React on a page in [`client`]; rendering to
//! HTML in [`server`] and [`prerender`] (`react-dom/static`).

use super::*;

/// [`createPortal`](https://react.dev/reference/react-dom/createPortal):
/// `children`, rendered into `container`, somewhere else in the DOM. Events
/// still bubble through the React tree.
#[rust_js::link_name = "react-dom#createPortal"]
pub fn create_portal(children: impl Node, container: &web::Element) -> Element {
    unreachable!()
}

/// `createPortal(children, container, key)`.
#[rust_js::link_name = "react-dom#createPortal"]
pub fn create_portal_with_key(children: impl Node, container: &web::Element, key: impl Key) -> Element {
    unreachable!()
}

/// [`flushSync`](https://react.dev/reference/react-dom/flushSync): apply the
/// updates in `f` to the DOM before returning.
#[rust_js::link_name = "react-dom#flushSync"]
pub fn flush_sync(f: impl FnOnce() + 'static) {
    unreachable!()
}

unsafe extern "Rust" {
    /// React DOM's version, like `"19.3.0"`.
    #[link_name = "react-dom#version"]
    pub safe static VERSION: &'static str;
}

// ── Resource hints ──────────────────────────────────────────────────────

/// [`prefetchDNS`](https://react.dev/reference/react-dom/prefetchDNS): look up
/// a server's IP address early.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react-dom#prefetchDNS"]
pub fn prefetch_dns(href: &str) {
    unreachable!()
}

/// [`preconnect`](https://react.dev/reference/react-dom/preconnect): connect
/// to a server early.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react-dom#preconnect"]
pub fn preconnect(href: &str) {
    unreachable!()
}

/// What a resource is, for [`preload`] and [`preinit`].
#[cfg(react = "19.0")]
pub enum As {
    #[rust_js::name = "audio"]
    Audio,
    #[rust_js::name = "document"]
    Document,
    #[rust_js::name = "embed"]
    Embed,
    #[rust_js::name = "fetch"]
    Fetch,
    #[rust_js::name = "font"]
    Font,
    #[rust_js::name = "image"]
    Image,
    #[rust_js::name = "object"]
    Object,
    #[rust_js::name = "script"]
    Script,
    #[rust_js::name = "style"]
    Style,
    #[rust_js::name = "track"]
    Track,
    #[rust_js::name = "video"]
    Video,
    #[rust_js::name = "worker"]
    Worker,
}

/// Declares an options object: `new(..)` makes it with its required fields,
/// and a method per optional one.
macro_rules! options {
    ($(#[doc = $doc:literal])* $(#[cfg($cfg:meta)])? $name:ident($new:literal $(, $arg:ident: $argty:ty)*) { $($(#[doc = $fdoc:literal])* $(#[cfg($fcfg:meta)])? $field:ident: $ty:ty = $js:literal;)* }) => {
        $(#[doc = $doc])*
        $(#[cfg($cfg)])?
        pub struct $name(PhantomData<JsObject>);

        $(#[cfg($cfg)])?
        impl $name {
            #[rust_js::link_name = $new]
            pub fn new($($arg: $argty),*) -> $name {
                unreachable!()
            }

            $(
                $(#[doc = $fdoc])*
                $(#[cfg($fcfg)])?
                #[rust_js::link_name = concat!("prop ", $js)]
                pub fn $field(self, value: $ty) -> $name {
                    unreachable!()
                }
            )*
        }
    };
}

options! {
    /// [`preload`]'s options: `PreloadOptions::new(As::Font)`.
    #[cfg(react = "19.0")]
    PreloadOptions("{as}", r#as: As) {
        /// `"anonymous"` or `"use-credentials"`; needed for `As::Fetch`.
        cross_origin: impl Value = "crossOrigin";
        referrer_policy: impl Value = "referrerPolicy";
        integrity: impl Value = "integrity";
        /// Its MIME type.
        r#type: impl Value = "type";
        nonce: impl Value = "nonce";
        /// `"auto"`, `"high"` or `"low"`.
        fetch_priority: impl Value = "fetchPriority";
        /// For `As::Image`.
        image_src_set: impl Value = "imageSrcSet";
        image_sizes: impl Value = "imageSizes";
    }
}

/// [`preload`](https://react.dev/reference/react-dom/preload): fetch a
/// resource you'll need soon.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react-dom#preload"]
pub fn preload(href: &str, options: PreloadOptions) {
    unreachable!()
}

options! {
    /// [`preload_module`]'s and [`preinit_module`]'s options.
    #[cfg(react = "19.0")]
    ModuleOptions("{}") {
        cross_origin: impl Value = "crossOrigin";
        integrity: impl Value = "integrity";
        nonce: impl Value = "nonce";
    }
}

/// [`preloadModule`](https://react.dev/reference/react-dom/preloadModule):
/// fetch an ES module you'll need soon.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react-dom#preloadModule"]
pub fn preload_module(href: &str, options: ModuleOptions) {
    unreachable!()
}

/// What [`preinit`] loads: a script or a stylesheet.
#[cfg(react = "19.0")]
pub enum Init {
    #[rust_js::name = "script"]
    Script,
    #[rust_js::name = "style"]
    Style,
}

options! {
    /// [`preinit`]'s options: `PreinitOptions::new(Init::Style).precedence("high")`.
    #[cfg(react = "19.0")]
    PreinitOptions("{as}", r#as: Init) {
        /// A stylesheet's place among others: `"reset"`, `"low"`, `"medium"`
        /// or `"high"`. A stylesheet needs one.
        precedence: impl Value = "precedence";
        cross_origin: impl Value = "crossOrigin";
        integrity: impl Value = "integrity";
        nonce: impl Value = "nonce";
        fetch_priority: impl Value = "fetchPriority";
    }
}

/// [`preinit`](https://react.dev/reference/react-dom/preinit): fetch and run
/// a script, or insert a stylesheet, early.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react-dom#preinit"]
pub fn preinit(href: &str, options: PreinitOptions) {
    unreachable!()
}

/// [`preinitModule`](https://react.dev/reference/react-dom/preinitModule):
/// fetch and run an ES module early.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react-dom#preinitModule"]
pub fn preinit_module(href: &str, options: ModuleOptions) {
    unreachable!()
}

// ── Forms ───────────────────────────────────────────────────────────────

/// [`useFormStatus`](https://react.dev/reference/react-dom/hooks/useFormStatus):
/// the last submission of the `<form>` this component is in.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react-dom#useFormStatus"]
pub fn use_form_status() -> &'static FormStatus {
    unreachable!()
}

/// What [`use_form_status`] gives.
#[cfg(react = "19.0")]
pub struct FormStatus(PhantomData<JsObject>);

#[cfg(react = "19.0")]
impl FormStatus {
    /// Whether the form is being submitted.
    #[rust_js::link_name = "get pending"]
    pub fn pending(&self) -> bool {
        unreachable!()
    }

    /// What it's submitting, while it is.
    #[rust_js::link_name = "get data"]
    pub fn data(&self) -> Option<&'static web::FormData> {
        unreachable!()
    }

    /// `"get"` or `"post"`.
    #[rust_js::link_name = "get method"]
    pub fn method(&self) -> String {
        unreachable!()
    }
}

/// [`requestFormReset`](https://react.dev/reference/react-dom/requestFormReset):
/// reset `form` once the current Transition is done.
#[cfg(react = "19.0")]
#[rust_js::link_name = "react-dom#requestFormReset"]
pub fn request_form_reset(form: &web::HtmlFormElement) {
    unreachable!()
}

/// What [`browser`] marks: content that only renders in the browser.
#[cfg(react = "19.3")]
pub struct Browser(PhantomData<JsObject>);

#[cfg(react = "19.3")]
impl Usable for Browser {
    type Output = ();
}

/// [`browser`](https://react.dev/reference/react-dom/browser): `use_(browser(reason))`
/// renders a component only in the browser. On the server, the nearest
/// [`suspense`]'s fallback shows instead, and `reason` says why.
#[cfg(react = "19.3")]
#[rust_js::link_name = "react-dom#browser"]
pub fn browser(reason: &str) -> Browser {
    unreachable!()
}

// ── react-dom/client ────────────────────────────────────────────────────

/// [`react-dom/client`](https://react.dev/reference/react-dom/client): putting
/// React on a page.
pub mod client {
    use super::*;

    /// Where React renders, made by [`create_root`] or [`hydrate_root`].
    pub struct Root(PhantomData<JsObject>);

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

    options! {
        /// A root's options: what to call on errors, and a prefix for [`use_id`]'s ids.
        RootOptions("{}") {
            /// An error an error boundary caught.
            on_caught_error: impl Fn(&Error, &ErrorInfo) + 'static = "onCaughtError";
            /// An error nothing caught.
            on_uncaught_error: impl Fn(&Error, &ErrorInfo) + 'static = "onUncaughtError";
            /// An error React recovered from, like a hydration mismatch.
            on_recoverable_error: impl Fn(&Error, &ErrorInfo) + 'static = "onRecoverableError";
            identifier_prefix: impl Value = "identifierPrefix";
        }
    }

    /// [`createRoot`](https://react.dev/reference/react-dom/client/createRoot).
    #[rust_js::link_name = "react-dom/client#createRoot"]
    pub fn create_root(container: &web::Element) -> &'static Root {
        unreachable!()
    }

    /// `createRoot(container, options)`.
    #[rust_js::link_name = "react-dom/client#createRoot"]
    pub fn create_root_with(container: &web::Element, options: RootOptions) -> &'static Root {
        unreachable!()
    }

    /// [`hydrateRoot`](https://react.dev/reference/react-dom/client/hydrateRoot):
    /// attach React to HTML the server rendered from `children`.
    #[rust_js::link_name = "react-dom/client#hydrateRoot"]
    pub fn hydrate_root(container: &web::Element, children: impl Node) -> &'static Root {
        unreachable!()
    }

    /// `hydrateRoot(container, children, options)`.
    #[rust_js::link_name = "react-dom/client#hydrateRoot"]
    pub fn hydrate_root_with(container: &web::Element, children: impl Node, options: RootOptions) -> &'static Root {
        unreachable!()
    }
}

// ── react-dom/server ────────────────────────────────────────────────────

/// [`react-dom/server`](https://react.dev/reference/react-dom/server):
/// rendering to HTML. Web streams (browsers, Deno, Bun, edge runtimes), or
/// Node's streams, or a string.
pub mod server {
    use super::*;

    options! {
        /// The options of [`render_to_string`] and [`render_to_static_markup`].
        StringOptions("{}") {
            identifier_prefix: impl Value = "identifierPrefix";
        }
    }

    /// [`renderToString`](https://react.dev/reference/react-dom/server/renderToString):
    /// HTML that [`client::hydrate_root`] can take over. A suspending
    /// component gets its fallback.
    #[rust_js::link_name = "react-dom/server#renderToString"]
    pub fn render_to_string(children: impl Node) -> String {
        unreachable!()
    }

    #[rust_js::link_name = "react-dom/server#renderToString"]
    pub fn render_to_string_with(children: impl Node, options: StringOptions) -> String {
        unreachable!()
    }

    /// [`renderToStaticMarkup`](https://react.dev/reference/react-dom/server/renderToStaticMarkup):
    /// HTML that won't be hydrated.
    #[rust_js::link_name = "react-dom/server#renderToStaticMarkup"]
    pub fn render_to_static_markup(children: impl Node) -> String {
        unreachable!()
    }

    #[rust_js::link_name = "react-dom/server#renderToStaticMarkup"]
    pub fn render_to_static_markup_with(children: impl Node, options: StringOptions) -> String {
        unreachable!()
    }

    options! {
        /// How to stream a page: the scripts that hydrate it, and what to
        /// call on errors.
        StreamOptions("{}") {
            bootstrap_script_content: impl Value = "bootstrapScriptContent";
            bootstrap_scripts: Vec<&'static str> = "bootstrapScripts";
            bootstrap_modules: Vec<&'static str> = "bootstrapModules";
            identifier_prefix: impl Value = "identifierPrefix";
            namespace_uri: impl Value = "namespaceURI";
            nonce: impl Value = "nonce";
            /// Called with each error on the server, recovered from or not.
            on_error: impl Fn(&Error) + 'static = "onError";
            progressive_chunk_size: u32 = "progressiveChunkSize";
            /// Stop rendering, and leave the rest to the client.
            signal: &'static web::AbortSignal = "signal";
            #[cfg(react = "19.0")]
            max_headers_length: u32 = "maxHeadersLength";
            /// Called with the `Link` headers for the page's preloads.
            #[cfg(react = "19.0")]
            on_headers: impl Fn(&web::Headers) + 'static = "onHeaders";
            /// Called when [`browser`] stops a component rendering here.
            #[cfg(react = "19.3")]
            on_browser_bailout: impl Fn(&Error, &ErrorInfo) + 'static = "onBrowserBailout";
        }
    }

    /// What [`render_to_readable_stream`] gives: a stream of the page's HTML.
    pub struct RenderStream(PhantomData<JsObject>);

    impl Deref for RenderStream {
        type Target = web::ReadableStream;

        fn deref(&self) -> &web::ReadableStream {
            // Never runs: rust-js compiles this `Deref` to the object itself.
            unsafe { &*(self as *const Self as *const web::ReadableStream) }
        }
    }

    impl RenderStream {
        /// Resolves once everything, suspended parts too, is rendered.
        #[rust_js::link_name = "get allReady"]
        pub fn all_ready(&self) -> Promise<()> {
            unreachable!()
        }
    }

    /// [`renderToReadableStream`](https://react.dev/reference/react-dom/server/renderToReadableStream):
    /// a Web stream of the page's HTML, sent as it's ready. It rejects if the
    /// page's shell fails.
    #[rust_js::link_name = "react-dom/server#renderToReadableStream"]
    pub fn render_to_readable_stream(children: impl Node, options: StreamOptions) -> Promise<&'static RenderStream> {
        unreachable!()
    }

    /// [`resume`](https://react.dev/reference/react-dom/server/resume): finish,
    /// as a Web stream, a page [`prerender`] postponed.
    #[cfg(react = "19.2")]
    #[rust_js::link_name = "react-dom/server#resume"]
    pub fn resume(children: impl Node, postponed: &Postponed, options: StreamOptions) -> Promise<&'static RenderStream> {
        unreachable!()
    }

    options! {
        /// [`StreamOptions`], for Node's streams, with when the page's shell
        /// and all of it are ready.
        PipeOptions("{}") {
            bootstrap_script_content: impl Value = "bootstrapScriptContent";
            bootstrap_scripts: Vec<&'static str> = "bootstrapScripts";
            bootstrap_modules: Vec<&'static str> = "bootstrapModules";
            identifier_prefix: impl Value = "identifierPrefix";
            namespace_uri: impl Value = "namespaceURI";
            nonce: impl Value = "nonce";
            on_error: impl Fn(&Error) + 'static = "onError";
            progressive_chunk_size: u32 = "progressiveChunkSize";
            /// When the shell is ready: time to [`pipe`](PipeableStream::pipe).
            on_shell_ready: impl Fn() + 'static = "onShellReady";
            on_shell_error: impl Fn(&Error) + 'static = "onShellError";
            /// When everything is ready, for crawlers and static pages.
            on_all_ready: impl Fn() + 'static = "onAllReady";
            #[cfg(react = "19.0")]
            max_headers_length: u32 = "maxHeadersLength";
            #[cfg(react = "19.3")]
            on_browser_bailout: impl Fn(&Error, &ErrorInfo) + 'static = "onBrowserBailout";
        }
    }

    /// What [`render_to_pipeable_stream`] gives.
    pub struct PipeableStream(PhantomData<JsObject>);

    impl PipeableStream {
        /// Send the HTML to a Node `Writable`, like an HTTP response.
        #[rust_js::link_name = "pipe"]
        pub fn pipe<W>(&self, destination: &W) {
            unreachable!()
        }

        /// Stop rendering, and leave the rest to the client.
        #[rust_js::link_name = "abort"]
        pub fn abort(&self) {
            unreachable!()
        }
    }

    /// [`renderToPipeableStream`](https://react.dev/reference/react-dom/server/renderToPipeableStream):
    /// the page's HTML, for Node's streams.
    #[rust_js::link_name = "react-dom/server#renderToPipeableStream"]
    pub fn render_to_pipeable_stream(children: impl Node, options: PipeOptions) -> &'static PipeableStream {
        unreachable!()
    }

    /// [`resumeToPipeableStream`](https://react.dev/reference/react-dom/server/resumeToPipeableStream):
    /// finish, for Node's streams, a page [`prerender`] postponed.
    #[cfg(react = "19.2")]
    #[rust_js::link_name = "react-dom/server#resumeToPipeableStream"]
    pub fn resume_to_pipeable_stream(children: impl Node, postponed: &Postponed, options: PipeOptions) -> Promise<&'static PipeableStream> {
        unreachable!()
    }

    /// What a prerender left for later: JSON, to keep until the request.
    pub struct Postponed(PhantomData<JsObject>);
}

// ── react-dom/static ────────────────────────────────────────────────────

/// [`react-dom/static`](https://react.dev/reference/react-dom/static):
/// rendering a whole page ahead of time, waiting for all of it.
pub mod prerender {
    #[cfg(react = "19.0")]
    use super::server::{Postponed, StreamOptions};
    #[cfg(react = "19.0")]
    use super::*;

    /// What a prerender gives: the HTML, and what it left for later.
    #[cfg(react = "19.0")]
    pub struct Prerendered(PhantomData<JsObject>);

    #[cfg(react = "19.0")]
    impl Prerendered {
        /// The HTML: a Web stream, or, from the `*_to_node_stream`
        /// functions, a Node `Readable`.
        #[rust_js::link_name = "get prelude"]
        pub fn prelude(&self) -> &'static web::ReadableStream {
            unreachable!()
        }

        /// What [`server::resume`] finishes, if anything was postponed.
        #[rust_js::link_name = "get postponed"]
        pub fn postponed(&self) -> Option<&'static Postponed> {
            unreachable!()
        }
    }

    /// [`prerender`](https://react.dev/reference/react-dom/static/prerender),
    /// with Web streams.
    #[cfg(react = "19.0")]
    #[rust_js::link_name = "react-dom/static#prerender"]
    pub fn prerender(children: impl Node, options: StreamOptions) -> Promise<&'static Prerendered> {
        unreachable!()
    }

    /// [`prerenderToNodeStream`](https://react.dev/reference/react-dom/static/prerenderToNodeStream),
    /// with Node's streams.
    #[cfg(react = "19.0")]
    #[rust_js::link_name = "react-dom/static#prerenderToNodeStream"]
    pub fn prerender_to_node_stream(children: impl Node, options: StreamOptions) -> Promise<&'static Prerendered> {
        unreachable!()
    }

    /// [`resumeAndPrerender`](https://react.dev/reference/react-dom/static/resumeAndPrerender):
    /// go on with a prerender that was postponed.
    #[cfg(react = "19.0")]
    #[rust_js::link_name = "react-dom/static#resumeAndPrerender"]
    pub fn resume_and_prerender(children: impl Node, postponed: &Postponed, options: StreamOptions) -> Promise<&'static Prerendered> {
        unreachable!()
    }

    /// [`resumeAndPrerenderToNodeStream`](https://react.dev/reference/react-dom/static/resumeAndPrerenderToNodeStream).
    #[cfg(react = "19.0")]
    #[rust_js::link_name = "react-dom/static#resumeAndPrerenderToNodeStream"]
    pub fn resume_and_prerender_to_node_stream(
        children: impl Node,
        postponed: &Postponed,
        options: StreamOptions,
    ) -> Promise<&'static Prerendered> {
        unreachable!()
    }
}
