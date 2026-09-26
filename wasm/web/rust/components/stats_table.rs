// Under the editors: how long loading took, and each compile.

use react::Element;

pub struct StatsTableProps {
    /// What was measured, and how it went.
    pub rows: &'static Vec<(String, String)>,
}

pub fn StatsTable(StatsTableProps { rows }: StatsTableProps) -> Element {
    jsx! {
        <table id="stats" className="mt-3">
            <tbody>
                {rows.iter().map(|(label, value)| jsx! {
                    <tr key={label.clone()}>
                        <td className="py-0.5 pr-4 tabular-nums text-muted">{label.clone()}</td>
                        <td className="py-0.5 pr-4 tabular-nums">{value.clone()}</td>
                    </tr>
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
