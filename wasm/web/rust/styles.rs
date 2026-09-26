// The Tailwind classes more than one component uses (ADR 0045). They're
// here, not in a component's file, so each of those exports only components.

/// The example picker and the buttons.
pub const CONTROL: &str = "rounded-md border border-line bg-panel px-3 py-1 enabled:cursor-pointer enabled:hover:bg-selected disabled:opacity-50";
pub const HEADING: &str = "mb-1.5 text-[13px] font-semibold text-muted";
/// A file explorer's row, or a folder's name, indented by `padding-left`.
pub const ROW: &str = "px-2 py-0.5";
