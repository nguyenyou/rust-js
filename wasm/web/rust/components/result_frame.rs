// The Result frame: the program's page, in a frame of its own, and what it
// reports back. A new frame each run, by its `key`: the program starts from a
// clean page, and a frame made while its section is showing is drawn at once.

use std::cell::Cell;
use std::rc::Rc;

use react::{Element, use_effect, use_ref};
use web::{Event, HtmlIFrameElement, JsObject, abort_controller, abort_signal, window};

use crate::listen::listen;
use crate::programs::{Outcome, Program, Report, outcome};
use crate::styles::HEADING;

// `setTimeout` is declared for this use only.
unsafe extern "Rust" {
    #[link_name = "setTimeout"]
    safe fn set_timeout(callback: Box<dyn FnOnce()>, ms: u32);
    #[link_name = "get contentWindow"]
    safe fn content_window(this: &HtmlIFrameElement) -> Option<&'static JsObject>;
    #[link_name = "get source"]
    safe fn message_source(this: &Event) -> Option<&'static JsObject>;
    #[link_name = "get data"]
    safe fn message_data(this: &Event) -> Option<Report>;
    #[link_name = "Object.is"]
    safe fn same_object(a: Option<&JsObject>, b: Option<&JsObject>) -> bool;
}

pub struct ResultFrameProps {
    /// What to run, if anything: without it, the section is hidden.
    pub program: &'static Option<Program>,
    pub on_outcome: Rc<dyn Fn(Outcome)>,
}

pub fn ResultFrame(ResultFrameProps { program, on_outcome }: ResultFrameProps) -> Element {
    let frame = use_ref(None::<&'static HtmlIFrameElement>);
    let (run, page) = match program {
        Some(program) => (program.run, program.page.clone()),
        None => (0, String::new()),
    };
    // Listen for this run's report. If there's none, say so: something
    // stopped its script.
    use_effect(
        move || {
            let controller = abort_controller::new();
            if run > 0 {
                let reported = Rc::new(Cell::new(false));
                let heard = reported.clone();
                let told = on_outcome.clone();
                listen(
                    window,
                    "message",
                    Box::new(move |e| {
                        let from_frame = match frame.current() {
                            Some(frame) => same_object(message_source(e), content_window(frame)),
                            None => false,
                        };
                        let report = match message_data(e) {
                            Some(report) if from_frame && report.run == Some(run) => report,
                            _ => return,
                        };
                        heard.set(true);
                        if let Some(outcome) = outcome(report) {
                            told(outcome);
                        }
                    }),
                    controller,
                );
                let signal = abort_controller::signal(controller);
                let silent = on_outcome.clone();
                set_timeout(
                    Box::new(move || {
                        if !abort_signal::aborted(signal) && !reported.get() {
                            silent(Outcome::Silent);
                        }
                    }),
                    3000,
                );
            }
            move || abort_controller::abort(controller)
        },
        (run,),
    );
    jsx! {
        <section id="result-section" className="mt-3" hidden={program.is_none()}>
            <h2 className={HEADING}>
                {"Result "}
                <span className="font-normal">
                    {"the root module's "}<code>{"main()"}</code>
                    {", or with Test its "}<code>{"#[test]"}</code>
                    {"s, in a frame of their own"}
                </span>
            </h2>
            <iframe
                key={run}
                ref={frame}
                id="result"
                className="block h-[280px] w-full rounded-md border border-line bg-page"
                title="Result"
                srcDoc={page}
            />
        </section>
    }
}
