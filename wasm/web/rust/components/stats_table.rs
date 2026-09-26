// Under the editors: how long loading took, and each compile.

use react::Element;
use react::html::{table, tbody, td, tr};

pub struct StatsTableProps {
    /// What was measured, and how it went.
    pub rows: &'static Vec<(String, String)>,
}

pub fn StatsTable(StatsTableProps { rows }: StatsTableProps) -> Element {
    table().id("stats").class_name("mt-3").children(
        tbody().children(
            rows.iter()
                .map(|(label, value)| {
                    tr().key(label.clone()).children((
                        td().class_name("py-0.5 pr-4 tabular-nums text-muted").children(label.clone()),
                        td().class_name("py-0.5 pr-4 tabular-nums").children(value.clone()),
                    ))
                })
                .collect::<Vec<_>>(),
        ),
    )
}
