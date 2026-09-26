// The page: its state, and the components it's made of (see ../components.rs).
//
// The state lives here, in hooks, and goes down as props. What happens goes
// up as callbacks, `on_compile`, `on_open`, which set it:
//
//   App ── status, project, output, program ──► Toolbar, Pane, ResultFrame
//       ◄── on_compile, on_open, on_delete, on_outcome ──┘

use std::cell::Cell;
use std::rc::Rc;

use react::{Element, use_effect, use_memo, use_ref, use_state, use_transition};
use web::{reg_exp, spawn, window};

use super::editor::Editor;
use super::file_tree::FileTree;
use super::pane::Pane;
use super::result_frame::ResultFrame;
use super::stats_table::StatsTable;
use super::status_line::{Status, Tone};
use super::toolbar::Toolbar;
use crate::codemirror::{EditorView, editor_state, output_state, source_state};
use crate::compiler::{
    Example, JsMap, Loaded, Stat, compile, load, load_example, mb, ms, set_last_result, text_entries,
};
use crate::programs::{Outcome, Prepared, Program, prepare};
use crate::projects::{Project, js_name};
use crate::tree::build_tree;

/// What the JavaScript side shows.
pub enum Output {
    Nothing,
    /// What the last compile wrote, `path → text`, and which file is showing.
    Files {
        files: Vec<(String, String)>,
        shown: String,
    },
    /// Why it failed: rustc's errors.
    Diagnostics(String),
}

fn say(text: String, tone: Tone) -> Status {
    Status { text, tone }
}

fn appended(rows: &Vec<(String, String)>, label: &str, value: &str) -> Vec<(String, String)> {
    let mut next: Vec<(String, String)> = rows.iter().map(|(l, v)| (l.clone(), v.clone())).collect();
    next.push((label.to_string(), value.to_string()));
    next
}

fn tests_summary(passed: u32, failed: u32, ignored: u32) -> Status {
    let total = passed + failed;
    let ignored_text = if ignored > 0 {
        format!(", {ignored} ignored")
    } else {
        String::new()
    };
    if total == 0 {
        say("No tests.".to_string(), Tone::Good)
    } else {
        let tone = if failed > 0 { Tone::Bad } else { Tone::Good };
        say(format!("Tests: {passed} passed, {failed} failed{ignored_text}."), tone)
    }
}

pub fn App() -> Element {
    let (loaded, set_loaded) = use_state(None::<Loaded>);
    let (stats, set_stats) = use_state(Vec::<(String, String)>::new());
    let (status, set_status) = use_state(say("Loading…".to_string(), Tone::Plain));
    let (example, set_example) = use_state(String::new());
    let (project, set_project) = use_state(Project::empty());
    let (output, set_output) = use_state(Output::Nothing);
    let (program, set_program) = use_state(None::<Program>);
    let (compiling, start_transition) = use_transition();
    // The source editor's view, whose state has the open file's latest edits.
    let source = use_ref(None::<&'static EditorView>);
    // How many programs have run, and how many compiles: a counter each.
    let runs = use_ref(0);
    let compiles = use_ref(0);

    // Download everything, then open the first example. In development,
    // StrictMode runs this twice; the first run's results are dropped.
    use_effect(
        move || {
            let cancelled = Rc::new(Cell::new(false));
            let dropped = cancelled.clone();
            let stat: Stat = Rc::new(move |label: String, value: String| {
                if !dropped.get() {
                    set_stats.update(move |rows| appended(rows, &label, &value));
                }
            });
            let done = cancelled.clone();
            spawn(Box::new(async move {
                let loaded = load(stat).await;
                let first = match loaded.examples.first() {
                    Some(e) => Some((e.name.clone(), e.root.clone(), e.files.clone())),
                    None => None,
                };
                if let Some((name, root, files)) = first
                    && !done.get()
                {
                    let texts = load_example(name.clone(), files).await;
                    if !done.get() {
                        set_example.set(name);
                        set_project.set(Project::of(root, texts));
                        set_loaded.set(Some(loaded));
                        set_status.set(say("Ready.".to_string(), Tone::Plain));
                    }
                }
            }));
            move || cancelled.set(true)
        },
        (),
    );

    // The open file's latest state, from its editor.
    let live = move || source.current().map(|view| editor_state(view));

    // Run what a compile wrote, in the Result frame.
    let run = move |files: &JsMap, root_js: &str, test: bool| {
        let n = runs.current() + 1;
        runs.set_current(n);
        match prepare(files, root_js, test, n) {
            Prepared::Nothing => set_program.set(None),
            Prepared::Jsx => {
                set_program.set(None);
                set_status.update(|s| say(format!("{} Preview React with the Vite example.", s.text), Tone::Plain));
            }
            Prepared::Blocked(imports) => {
                set_program.set(None);
                let names: Vec<String> = imports.iter().map(|s| format!("\"{s}\"")).collect();
                let names = names.join(", ");
                set_status.update(move |s| {
                    let text = format!(
                        "{} Not run: it imports {names}, which the playground can't load. Bundle it with bun build.",
                        s.text
                    );
                    say(text, Tone::Bad)
                });
            }
            Prepared::Page(page) => set_program.set(Some(Program { run: n, page })),
        }
    };

    // Compile the crate, or with `test`, its tests too, and run it.
    let on_compile: Rc<dyn Fn(bool)> = Rc::new(move |test: bool| {
        let loaded = match loaded {
            Some(loaded) => loaded,
            None => return,
        };
        if compiling {
            return;
        }
        let sources = project.sources(live());
        let root = project.root.clone();
        let root_js = js_name(&project.root);
        let shown = match output {
            Output::Files { shown, .. } => shown.clone(),
            _ => String::new(),
        };
        set_status.set(say(
            if test { "Compiling the tests…" } else { "Compiling…" }.to_string(),
            Tone::Plain,
        ));
        // A Transition: `compiling` is true until it's done.
        start_transition.start(move || async move {
            let r = compile(loaded, sources, &root, test).await;
            let n = compiles.current() + 1;
            compiles.set_current(n);
            if r.ok {
                let files = text_entries(r.files);
                let count = files.len();
                let root_jsx = format!("{root_js}x");
                let root_js = if files.iter().any(|(path, _)| *path == root_jsx) {
                    root_jsx
                } else {
                    root_js
                };
                // Keep showing the same file if it's still there; otherwise the root's.
                let shown = if files.iter().any(|(path, _)| *path == shown) {
                    shown
                } else {
                    root_js.clone()
                };
                set_output.set(Output::Files { files, shown });
                set_status.set(say(
                    format!("Compiled: {count} JS file{}.", if count == 1 { "" } else { "s" }),
                    Tone::Good,
                ));
                run(r.files, &root_js, test);
            } else {
                set_output.set(Output::Diagnostics(r.stderr.clone()));
                set_program.set(None);
                set_status.set(say(format!("Failed: exit {}.", r.exit), Tone::Bad));
            }
            let result = if r.ok { "ok" } else { "error" };
            let times = format!(
                "instantiate {}, run {}, memory {}, {result}",
                ms(r.instantiate),
                ms(r.run),
                mb(r.memory as f64)
            );
            let label = format!("compile #{n}");
            set_stats.update(move |rows| appended(rows, &label, &times));
            // For automated checks.
            set_last_result(window, &r);
        });
    });

    // What the Result frame reports.
    let on_outcome: Rc<dyn Fn(Outcome)> = Rc::new(move |outcome: Outcome| match outcome {
        Outcome::Failed(error) => set_status.set(say(format!("Runtime error: {error}"), Tone::Bad)),
        Outcome::Ran => set_status.update(|s| say(format!("{} Ran main().", s.text), Tone::Good)),
        Outcome::Tested(t) => set_status.set(tests_summary(t.passed, t.failed, t.ignored)),
        Outcome::Silent => {
            let text = "The Result frame didn't run. Is something blocking its script? See the console.";
            set_status.set(say(text.to_string(), Tone::Bad))
        }
    });

    let on_example: Rc<dyn Fn(String)> = Rc::new(move |name: String| {
        let chosen: Option<&Example> = match loaded {
            Some(loaded) => loaded.examples.iter().find(|e| e.name == name),
            None => None,
        };
        if let Some(chosen) = chosen {
            let (root, files) = (chosen.root.clone(), chosen.files.clone());
            set_example.set(name.clone());
            spawn(Box::new(async move {
                let texts = load_example(name, files).await;
                set_project.set(Project::of(root, texts));
                set_output.set(Output::Nothing);
                set_program.set(None);
                set_status.set(say("Ready.".to_string(), Tone::Plain));
            }));
        }
    });

    let open_file: Rc<dyn Fn(String)> = Rc::new(move |path: String| set_project.set(project.opening(&path, live())));
    let delete_file: Rc<dyn Fn(String)> = Rc::new(move |path: String| {
        if window::confirm_with_message(window, &format!("Delete {path}?")) {
            set_project.set(project.removing(&path));
        }
    });
    let new_file = move || {
        let answer = match window::prompt_with_message(window, "New file, e.g. math.rs or geometry/shape.rs:") {
            Some(answer) => answer,
            None => return,
        };
        let path = answer.trim().to_string();
        if path.is_empty() {
            return;
        }
        let module_path = reg_exp::new(r"^([a-z_][a-z0-9_]*/)*[a-z_][a-z0-9_]*\.rs$", "");
        if !reg_exp::test(module_path, &path) {
            let text = format!("\"{path}\" isn't a Rust module file name, like math.rs or geometry/shape.rs.");
            set_status.set(say(text, Tone::Bad));
            return;
        }
        if project.has(&path) {
            set_status.set(say(format!("{path} already exists."), Tone::Bad));
            return;
        }
        set_project.set(project.adding(&path, live()));
        let file = match path.rsplit_once('/') {
            Some((_, file)) => file,
            None => path.as_str(),
        };
        let module = file.strip_suffix(".rs").unwrap_or(file);
        let text = format!("Created {path}. Declare it with `mod {module};` in its parent, or rustc won't include it.");
        set_status.set(say(text, Tone::Plain));
    };
    let open_output: Rc<dyn Fn(String)> = Rc::new(move |path: String| {
        if let Output::Files { files, .. } = output {
            let files = files.iter().map(|(p, t)| (p.clone(), t.clone())).collect();
            set_output.set(Output::Files { files, shown: path });
        }
    });

    // What the editors show. A state is made once for each output, so the
    // editor only changes when it does.
    let blank = *use_memo(|| source_state(""), ());
    let current = match project.current_state() {
        Some(state) => state,
        None => blank,
    };
    let shown_state = *use_memo(
        move || match output {
            Output::Nothing => output_state("", true),
            Output::Files { files, shown } => {
                let text = match files.iter().find(|(path, _)| path == shown) {
                    Some((_, text)) => text.clone(),
                    None => String::new(),
                };
                output_state(&text, true)
            }
            Output::Diagnostics(text) => output_state(text, false),
        },
        (output,),
    );
    let source_tree = use_memo(move || build_tree(&project.paths()), (project,));
    let (output_paths, shown) = match output {
        Output::Files { files, shown } => (files.iter().map(|(path, _)| path.clone()).collect(), shown.clone()),
        _ => (Vec::new(), String::new()),
    };
    let output_tree = use_memo(move || build_tree(&output_paths), (output,));
    let examples: &'static [Example] = match loaded {
        Some(loaded) => &loaded.examples,
        None => &[],
    };
    let submit = on_compile.clone();

    jsx! {
        <>
            <h1 className="mb-1 text-lg font-bold">{"rust-js playground"}</h1>
            <p className="mb-3 text-muted">{"rustc's front end and rust-js, as WebAssembly. No server compiles anything."}</p>
            <Toolbar
                examples={examples}
                example={example.clone()}
                onExample={on_example}
                ready={loaded.is_some() && !compiling}
                onCompile={on_compile}
                status={status}
            />
            <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,440px),1fr))] gap-3">
                <Pane
                    title="Rust"
                    label="Rust files"
                    explorer={jsx! {
                        <>
                            <ul id="source-files">
                                <FileTree
                                    tree={source_tree}
                                    depth={0}
                                    first={project.root.clone()}
                                    selected={project.current.clone()}
                                    onOpen={open_file}
                                    onDelete={Some(delete_file)}
                                />
                            </ul>
                            <button
                                id="new-file"
                                className="mx-2 mt-1.5 block cursor-pointer text-muted"
                                onClick={move |_| new_file()}
                            >
                                {"+ New file"}
                            </button>
                        </>
                    }}
                    editor={jsx! {
                        <Editor
                            state={current}
                            view={Some(source)}
                            onSubmit={Some(Rc::new(move || submit(false)))}
                        />
                    }}
                />
                <Pane
                    title="JavaScript"
                    label="JavaScript files"
                    explorer={jsx! {
                        <ul id="output-files">
                            {if output_tree.is_empty() {
                                jsx! { <li className="flex items-center px-2 py-0.5 text-muted">{"(none)"}</li> }
                            } else {
                                jsx! {
                                    <FileTree
                                        tree={output_tree}
                                        depth={0}
                                        first={js_name(&project.root)}
                                        selected={shown}
                                        onOpen={open_output}
                                        onDelete={None}
                                    />
                                }
                            }}
                        </ul>
                    }}
                    editor={jsx! { <Editor state={shown_state} view={None} onSubmit={None} /> }}
                />
            </div>
            <ResultFrame program={program} onOutcome={on_outcome} />
            <StatsTable rows={stats} />
        </>
    }
}
