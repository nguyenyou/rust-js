// One side of the page: a heading, and a file explorer beside an editor.

use react::Element;
use react::html::{div, h2, nav, section};

use crate::styles::HEADING;

pub struct PaneProps {
    pub title: &'static str,
    /// What the explorer is, for a screen reader: "Rust files".
    pub label: &'static str,
    pub explorer: Element,
    pub editor: Element,
}

pub fn Pane(PaneProps { title, label, explorer, editor }: PaneProps) -> Element {
    section().children((
        h2().class_name(HEADING).children(title),
        div().class_name("grid h-[460px] grid-cols-[150px_minmax(0,1fr)] overflow-hidden rounded-md border border-line").children((
            nav()
                .class_name("overflow-auto border-r border-line bg-panel py-1.5 font-mono text-[13px]")
                .attr("aria-label", label)
                .children(explorer),
            editor,
        )),
    ))
}
