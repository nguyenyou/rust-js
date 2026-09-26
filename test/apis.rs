// React's and React DOM's APIs beyond the basics (ADR 0043), each in a
// component that test/apis.jsx renders with React 19.3 and checks.

#![allow(non_snake_case)]

use react::dom::client::{Root, RootOptions, create_root_with};
use react::dom::server::{
    RenderStream, StreamOptions, StringOptions, render_to_readable_stream, render_to_string_with,
};
use react::dom::{create_portal, flush_sync, use_form_status};
use react::web;
use react::{
    Activity, ActivityMode, Element, Lazy, Module, Phase, Ref, Style, import_module, inner_html, lazy, use_,
    use_action_state, use_deferred_value, use_effect, use_effect_event, use_id, use_imperative_handle,
    use_layout_effect, use_optimistic, use_reducer_with, use_ref, use_state, use_sync_external_store, use_transition,
};
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
    jsx! {
        <p className="greeting">{text}</p>
    }
}

pub fn Suspended() -> Element {
    jsx! {
        <Suspense
            fallback={jsx! {
                <p className="loading">{"Loading"}</p>
            }}>
            <Greeting />
        </Suspense>
    }
}

/// A store outside React, and a Transition.
pub fn Store() -> Element {
    let value = use_sync_external_store(|notify| store_subscribe(notify), || store_get());
    let (pending, start) = use_transition();
    let (filter, set_filter) = use_state(0);
    let deferred = use_deferred_value(*filter);
    jsx! {
        <div>
            <span className="store">{value}</span>
            <span className="deferred">{deferred}</span>
            <span className="pending">{if pending { "pending" } else { "idle" }}</span>
            <button className="transition" onClick={move |_| start.start(move || set_filter.set(7))}>{"go"}</button>
        </div>
    }
}

/// A form with an action, its state, an optimistic value, and its status.
fn submit(previous: &String, data: &'static FormData) -> String {
    let name = form_data::get(data, "name").unwrap_or(String::new());
    format!("{previous}{name};")
}

pub fn SubmitStatus() -> Element {
    let status = use_form_status();
    jsx! {
        <span className="status">{if status.pending() { "sending" } else { "ready" }}</span>
    }
}

pub fn Signup() -> Element {
    let (names, action, pending) = use_action_state(submit, String::new());
    let (optimistic, _set_optimistic) = use_optimistic(names.clone());
    jsx! {
        <form action={action}>
            <input name="name" defaultValue="ada" />
            <button type="submit">{"Sign up"}</button>
            <SubmitStatus />
            <span className="names">{optimistic}</span>
            <span className="action-pending">{if pending { "yes" } else { "no" }}</span>
        </form>
    }
}

/// Refs: a DOM element, an imperative handle, and a ref callback's cleanup.
pub struct FancyInputProps {
    pub handle: Ref<Option<&'static str>>,
}

pub fn FancyInput(FancyInputProps { handle }: FancyInputProps) -> Element {
    use_imperative_handle(handle, || "handle from FancyInput", ());
    jsx! {
        <input className="fancy" />
    }
}

pub fn Refs() -> Element {
    let handle: Ref<Option<&'static str>> = use_ref(None);
    let element: Ref<Option<&'static web::Element>> = use_ref(None);
    let (shown, set_shown) = use_state(true);
    use_layout_effect(move || log("layout"), ());
    use_effect(
        move || {
            log(if element.current().is_some() {
                "element set"
            } else {
                "no element"
            });
            log(handle.current().unwrap_or("no handle"));
        },
        (),
    );
    let on_log = use_effect_event(move || log(if *shown { "shown" } else { "hidden" }));
    use_effect(move || on_log(), (shown,));
    jsx! {
        <div>
            <FancyInput handle={handle} />
            <p ref={element}>{"with a ref"}</p>
            {if *shown {
                Some(jsx! {
                    <b
                        ref={move |node: Option<&'static web::Element>| {
                            log(if node.is_some() { "attached" } else { "null" });
                            move || log("detached")
                        }}>
                        {"callback"}
                    </b>
                })
            } else {
                None
            }}
            <button className="hide" onClick={move |_| set_shown.set(false)}>{"hide"}</button>
        </div>
    }
}

/// Activity keeps hidden state; a portal renders elsewhere; flushSync applies now.
pub fn Counter() -> Element {
    let (n, set_n) = use_state(0);
    jsx! {
        <button className="count" onClick={move |_| set_n.update(|n| n + 1)}>{n}</button>
    }
}

pub fn Places() -> Element {
    let (hidden, set_hidden) = use_state(false);
    let (flushed, set_flushed) = use_state(0);
    jsx! {
        <>
            <Activity mode={if *hidden { ActivityMode::Hidden } else { ActivityMode::Visible }}>
                <Counter />
            </Activity>
            <button className="toggle" onClick={move |_| set_hidden.update(|h| !h)}>{"toggle"}</button>
            {create_portal(jsx! {
                <span className="portaled">{"in the portal"}</span>
            }, portal_target)}
            <button
                className="flush"
                onClick={move |_| {
                    flush_sync(move || set_flushed.set(1));
                    // flushSync has already put the new count on the page.
                    log(&format!("flushed {}", flushed_text()));
                }}>
                {flushed}
            </button>
        </>
    }
}

// Keyed fragments, styles, raw HTML, any attribute, a lazy and
// a reducer with an initializer, under a Profiler.
thread_local! {
    static LAZY_CARD: Lazy<()> = lazy(|| import_module::<()>("./lazy-card.jsx"));
}

pub fn Misc() -> Element {
    let id = use_id();
    let (total, _dispatch) = use_reducer_with(|s: &i32, a: i32| s + a, 20, |start| start * 2 + 2);
    jsx! {
        <Profiler
            id="misc"
            onRender={|id, phase, _, _, _, _| log(&format!("{id} {}", match phase { Phase::Mount => "mount", Phase::Update => "update", Phase::NestedUpdate => "nested" }))}>
            <ul>
                {vec![1, 2].into_iter().map(|n| jsx! {
                    <Fragment key={n}>
                        <li>{n}</li>
                        <li>{"·"}</li>
                    </Fragment>
                }).collect::<Vec<_>>()}
            </ul>
            <div className="styled" style={Style::new().color("red").font_size(12).set("--gap", "4px")} />
            <div className="raw" dangerouslySetInnerHtml={inner_html("<i>raw</i>")} />
            <span className="id" data-id={id}>{total}</span>
            <Suspense fallback="loading card">
                <LAZY_CARD />
            </Suspense>
        </Profiler>
    }
}

#[allow(dead_code)]
fn unused(_: Activity, _: Module<()>) {}

/// Putting React on a page, and rendering it on a server, with options.
pub fn Page() -> Element {
    jsx! {
        <p className="page">
            {"id "}
            {use_id()}
        </p>
    }
}

pub fn page_html() -> String {
    render_to_string_with(
        jsx! {
            <Page />
        },
        StringOptions::new().identifier_prefix("s-"),
    )
}

pub async fn page_stream() -> &'static RenderStream {
    let stream = render_to_readable_stream(
        jsx! {
            <Page />
        },
        StreamOptions::new().identifier_prefix("w-"),
    )
    .await;
    stream.all_ready().await;
    stream
}

pub fn mount(container: &web::Element) -> &'static Root {
    let root = create_root_with(container, RootOptions::new().identifier_prefix("c-"));
    root.render(jsx! {
        <Page />
    });
    root
}
