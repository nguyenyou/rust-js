// The Result frame: the program's page, in a frame of its own, and what it
// reports back. A new frame each run, by its `key`: the program starts from a
// clean page, and a frame made while its section is showing is drawn at once.

use std::cell::Cell;
use std::rc::Rc;

use js::{object, set_timeout};
use react::{Element, use_effect, use_ref};
use webapi::{
    Event, HtmlIFrameElement, abort_controller, abort_signal, element, html_i_frame_element, message_event, window,
};

use crate::listen::listen;
use crate::programs::{Outcome, Program, Report, outcome};
use crate::styles::HEADING;

// The frame's report, which frame-report.js reads.
unsafe extern "Rust" {
    #[link_name = "../frame-report.js#readReport"]
    safe fn message_data(event: &Event) -> Option<Report>;
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
                            Some(frame) => object::is(
                                &message_event::source(message_event::unchecked_from(e)),
                                &html_i_frame_element::content_window(frame),
                            ),
                            None => false,
                        };
                        let report = match message_data(e) {
                            Some(report) if from_frame && report.run == Some(run) => report,
                            _ => return,
                        };
                        // As tall as its page, from the height it starts with.
                        if let (Some(height), Some(frame)) = (report.height, frame.current()) {
                            let style = format!("height: {}px", height.max(280.0).min(4000.0));
                            element::set_attribute(frame, "style", &style);
                            return;
                        }
                        if let Some(outcome) = outcome(report) {
                            heard.set(true);
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
                sandbox="allow-scripts"
                srcDoc={page}
            />
        </section>
    }
}
