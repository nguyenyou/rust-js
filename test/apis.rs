// React's and React DOM's APIs beyond the basics (ADR 0043), each in a
// component that test/apis.jsx renders with React 19.3 and checks.

#![allow(non_snake_case)]

use react::dom::client::{Root, RootOptions, create_root_with};
use react::dom::server::{RenderStream, StreamOptions, StringOptions, render_to_readable_stream, render_to_string_with};
use react::dom::{create_portal, flush_sync, use_form_status};
use react::html::{b, button, div, form, input, li, p, span, ul};
use react::{
    Activity, ActivityMode, Element, Lazy, Module, Phase, Ref, Style, activity, component, fragment, import_module,
    inner_html, keyed_fragment, lazy, profiler, suspense, use_, use_action_state, use_deferred_value,
    use_effect, use_effect_event, use_id, use_imperative_handle, use_layout_effect, use_optimistic, use_reducer_with,
    use_ref, use_state, use_sync_external_store, use_transition,
};
use react::web;
use web::{FormData, Promise, form_data};

unsafe extern "Rust" {
    /// The test's own JS: a promise, a store, and where things are logged.
    #[link_name = "globalThis.greeting"]
    safe static greeting: &'static Promise<String>;
    #[link_name = "globalThis.store.subscribe"]
    safe fn store_subscribe(notify: react::Notify) -> Box<dyn FnOnce()>;
    #[link_name = "globalThis.store.get"]
    safe fn store_get() -> i32;
    #[link_name = "globalThis.log"]
    safe fn log(what: &str);
    #[link_name = "globalThis.portalTarget"]
    safe static portal_target: &'static web::Element;
    #[link_name = "globalThis.flushedText"]
    safe fn flushed_text() -> String;
}

/// `use` a promise: Suspense shows the fallback until it resolves.
pub fn Greeting() -> Element {
    let text = use_(greeting);
    p().class_name("greeting").children(text)
}

pub fn Suspended() -> Element {
    suspense().fallback(p().class_name("loading").children("Loading")).children(component(Greeting, ()))
}

/// A store outside React, and a Transition.
pub fn Store() -> Element {
    let value = use_sync_external_store(|notify| store_subscribe(notify), || store_get());
    let (pending, start) = use_transition();
    let (filter, set_filter) = use_state(0);
    let deferred = use_deferred_value(*filter);
    div().children((
        span().class_name("store").children(value),
        span().class_name("deferred").children(deferred),
        span().class_name("pending").children(if pending { "pending" } else { "idle" }),
        button().class_name("transition").on_click(move |_| start.start(move || set_filter.set(7))).children("go"),
    ))
}

/// A form with an action, its state, an optimistic value, and its status.
fn submit(previous: &String, data: &'static FormData) -> String {
    let name = form_data::get(data, "name").unwrap_or(String::new());
    format!("{previous}{name};")
}

pub fn SubmitStatus() -> Element {
    let status = use_form_status();
    span().class_name("status").children(if status.pending() { "sending" } else { "ready" })
}

pub fn Signup() -> Element {
    let (names, action, pending) = use_action_state(submit, String::new());
    let (optimistic, _set_optimistic) = use_optimistic(names.clone());
    form().action_dispatch(action).children((
        input().name("name").default_value("ada"),
        button().r#type("submit").children("Sign up"),
        component(SubmitStatus, ()),
        span().class_name("names").children(optimistic),
        span().class_name("action-pending").children(if pending { "yes" } else { "no" }),
    ))
}

/// Refs: a DOM element, an imperative handle, and a ref callback's cleanup.
pub struct FancyInputProps {
    pub handle: Ref<Option<&'static str>>,
}

pub fn FancyInput(FancyInputProps { handle }: FancyInputProps) -> Element {
    use_imperative_handle(handle, || "handle from FancyInput", ());
    input().class_name("fancy")
}

pub fn Refs() -> Element {
    let handle: Ref<Option<&'static str>> = use_ref(None);
    let element: Ref<Option<&'static web::Element>> = use_ref(None);
    let (shown, set_shown) = use_state(true);
    use_layout_effect(move || log("layout"), ());
    use_effect(
        move || {
            log(if element.current().is_some() { "element set" } else { "no element" });
            log(handle.current().unwrap_or("no handle"));
        },
        (),
    );
    let on_log = use_effect_event(move || log(if *shown { "shown" } else { "hidden" }));
    use_effect(move || on_log(), (shown,));
    div().children((
        component(FancyInput, FancyInputProps { handle }),
        p().r#ref(element).children("with a ref"),
        if *shown {
            Some(b().ref_callback(move |node: Option<&'static web::Element>| {
                log(if node.is_some() { "attached" } else { "null" });
                move || log("detached")
            }).children("callback"))
        } else {
            None
        },
        button().class_name("hide").on_click(move |_| set_shown.set(false)).children("hide"),
    ))
}

/// Activity keeps hidden state; a portal renders elsewhere; flushSync applies now.
pub fn Counter() -> Element {
    let (n, set_n) = use_state(0);
    button().class_name("count").on_click(move |_| set_n.update(|n| n + 1)).children(n)
}

pub fn Places() -> Element {
    let (hidden, set_hidden) = use_state(false);
    let (flushed, set_flushed) = use_state(0);
    fragment((
        activity().mode(if *hidden { ActivityMode::Hidden } else { ActivityMode::Visible }).children(component(Counter, ())),
        button().class_name("toggle").on_click(move |_| set_hidden.update(|h| !h)).children("toggle"),
        create_portal(span().class_name("portaled").children("in the portal"), portal_target),
        button()
            .class_name("flush")
            .on_click(move |_| {
                flush_sync(move || set_flushed.set(1));
                // flushSync has already put the new count on the page.
                log(&format!("flushed {}", flushed_text()));
            })
            .children(flushed),
    ))
}

// Keyed fragments, styles, raw HTML, any attribute, a lazy component, and
// a reducer with an initializer, under a Profiler.
thread_local! {
    static LAZY_CARD: Lazy<()> = lazy(|| import_module::<()>("./lazy-card.jsx"));
}

pub fn Misc() -> Element {
    let id = use_id();
    let (total, _dispatch) = use_reducer_with(|s: &i32, a: i32| s + a, 20, |start| start * 2 + 2);
    profiler()
        .id("misc")
        .on_render(|id, phase, _, _, _, _| log(&format!("{id} {}", match phase { Phase::Mount => "mount", Phase::Update => "update", Phase::NestedUpdate => "nested" })))
        .children((
            ul().children(vec![1, 2].into_iter().map(|n| keyed_fragment().key(n).children((li().children(n), li().children("·")))).collect::<Vec<_>>()),
            div().class_name("styled").style(Style::new().color("red").font_size(12).set("--gap", "4px")),
            div().class_name("raw").dangerously_set_inner_html(inner_html("<i>raw</i>")),
            span().class_name("id").attr("data-id", id).children(total),
            suspense().fallback("loading card").children(component(&LAZY_CARD, ())),
        ))
}

#[allow(dead_code)]
fn unused(_: Activity, _: Module<()>) {}

/// Putting React on a page, and rendering it on a server, with options.
pub fn Page() -> Element {
    p().class_name("page").children(("id ", use_id()))
}

pub fn page_html() -> String {
    render_to_string_with(component(Page, ()), StringOptions::new().identifier_prefix("s-"))
}

pub async fn page_stream() -> &'static RenderStream {
    let stream = render_to_readable_stream(component(Page, ()), StreamOptions::new().identifier_prefix("w-")).await;
    stream.all_ready().await;
    stream
}

pub fn mount(container: &web::Element) -> &'static Root {
    let root = create_root_with(container, RootOptions::new().identifier_prefix("c-"));
    root.render(component(Page, ()));
    root
}
