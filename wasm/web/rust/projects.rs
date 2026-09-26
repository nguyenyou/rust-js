// The crate being edited: its files, its root, and the one that's open.
//
// React state doesn't change: each edit here makes a new `Project`, and
// `App` renders with it. In JS, the methods are `Project`'s object of them
// (ADR 0047): `project.opening(path, live)` is `Project.opening(project, path, live)`.

use crate::codemirror::{EditorState, source_state, text_of};
use crate::compiler::{JsMap, new_text_map};

/// A Rust file, and its editor state: its text, selection and undo history.
pub struct SourceFile {
    pub path: String,
    pub state: &'static EditorState,
}

pub struct Project {
    pub root: String,
    pub files: Vec<SourceFile>,
    pub current: String,
}

fn copy(files: &[SourceFile]) -> Vec<SourceFile> {
    files
        .iter()
        .map(|f| SourceFile {
            path: f.path.clone(),
            state: f.state,
        })
        .collect()
}

/// The file that becomes the root module's JS, `lib.rs` → `lib.js`.
pub fn js_name(path: &str) -> String {
    match path.strip_suffix(".rs") {
        Some(stem) => format!("{stem}.js"),
        None => path.to_string(),
    }
}

impl Project {
    /// Before an example has loaded.
    pub fn empty() -> Project {
        Project {
            root: "lib.rs".to_string(),
            files: Vec::new(),
            current: String::new(),
        }
    }

    /// An example's files, with its root open.
    pub fn of(root: String, texts: Vec<(String, String)>) -> Project {
        let files = texts
            .iter()
            .map(|(path, text)| SourceFile {
                path: path.clone(),
                state: source_state(text),
            })
            .collect();
        Project {
            current: root.clone(),
            root,
            files,
        }
    }

    pub fn has(&self, path: &str) -> bool {
        self.files.iter().any(|f| f.path == path)
    }

    pub fn paths(&self) -> Vec<String> {
        self.files.iter().map(|f| f.path.clone()).collect()
    }

    /// The open file's state, as it was stored: its editor has the latest.
    pub fn current_state(&self) -> Option<&'static EditorState> {
        match self.files.iter().find(|f| f.path == self.current) {
            Some(file) => Some(file.state),
            None => None,
        }
    }

    /// With the open file's `live` state kept, since the editor has it.
    fn keeping(&self, live: Option<&'static EditorState>) -> Vec<SourceFile> {
        let mut files = copy(&self.files);
        if let Some(live) = live {
            for file in files.iter_mut() {
                if file.path == self.current {
                    file.state = live;
                }
            }
        }
        files
    }

    /// `path` open instead.
    pub fn opening(&self, path: &str, live: Option<&'static EditorState>) -> Project {
        Project {
            root: self.root.clone(),
            files: self.keeping(live),
            current: path.to_string(),
        }
    }

    /// With a new, empty file at `path`, open.
    pub fn adding(&self, path: &str, live: Option<&'static EditorState>) -> Project {
        let mut files = self.keeping(live);
        files.push(SourceFile {
            path: path.to_string(),
            state: source_state(""),
        });
        Project {
            root: self.root.clone(),
            files,
            current: path.to_string(),
        }
    }

    /// Without `path`; if it was open, the root is.
    pub fn removing(&self, path: &str) -> Project {
        let files = self
            .files
            .iter()
            .filter(|f| f.path != path)
            .map(|f| SourceFile {
                path: f.path.clone(),
                state: f.state,
            })
            .collect();
        let current = if self.current == path {
            self.root.clone()
        } else {
            self.current.clone()
        };
        Project {
            root: self.root.clone(),
            files,
            current,
        }
    }

    /// The crate as text, `path → text`, with the open file's `live` edits.
    pub fn sources(&self, live: Option<&'static EditorState>) -> &'static JsMap {
        let files = self.keeping(live);
        new_text_map(files.iter().map(|f| (f.path.clone(), text_of(f.state))).collect())
    }
}
