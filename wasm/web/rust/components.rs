// The page's components, one per file (ADR 0044). Each file is a JS module
// (ADR 0019) that exports only components, so saving one is a Fast Refresh.
//
//   App                      the state, and the page's layout
//   ├── Toolbar              the example, Compile and Test, the status
//   │   ├── ExamplePicker
//   │   └── StatusLine
//   ├── Pane "Rust"          a file explorer beside an editor
//   │   ├── FileTree         a folder's entries, and each folder's, inside
//   │   │   └── FileItem     a file: open it, or delete it
//   │   └── Editor           CodeMirror, made in an effect
//   ├── Pane "JavaScript"
//   │   ├── FileTree
//   │   └── Editor
//   ├── ResultFrame          runs the program, and hears how it went
//   └── StatsTable

pub mod app;
pub mod editor;
pub mod example_picker;
pub mod file_item;
pub mod file_tree;
pub mod pane;
pub mod result_frame;
pub mod stats_table;
pub mod status_line;
pub mod toolbar;
