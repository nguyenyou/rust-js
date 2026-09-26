// A CodeMirror editor. React renders its element; the view inside is
// CodeMirror's, made when the component mounts and destroyed when it goes.

use std::rc::Rc;

use react::event::Keyboard;
use react::{Element, Ref, use_effect, use_ref};

use crate::codemirror::{EditorState, EditorView, destroy, open_view, set_theme, show};
use crate::dark_mode::use_dark_mode;

pub struct EditorProps {
    /// What it shows: a file's state, with its text and undo history.
    pub state: &'static EditorState,
    /// Where a parent that reads the editor gets its view.
    pub view: Option<Ref<Option<&'static EditorView>>>,
    /// What ⌘/Ctrl-Enter does.
    pub on_submit: Option<Rc<dyn Fn()>>,
}

pub fn Editor(EditorProps { state, view, on_submit }: EditorProps) -> Element {
    let parent = use_ref(None::<&'static web::Element>);
    let made = use_ref(None::<&'static EditorView>);
    let dark = use_dark_mode();
    use_effect(
        move || {
            let editor = open_view(parent.current().expect("the editor's element is mounted"), state);
            made.set_current(Some(editor));
            if let Some(view) = view {
                view.set_current(Some(editor));
            }
            move || {
                destroy(editor);
                made.set_current(None);
                if let Some(view) = view {
                    view.set_current(None);
                }
            }
        },
        (),
    );
    // Another file, a new compile, or the system's theme changing.
    use_effect(
        move || {
            if let Some(editor) = made.current() {
                show(editor, state);
                set_theme(editor, dark);
            }
        },
        (state, dark),
    );
    jsx! {
        <div
            className="min-w-0 overflow-hidden [&_.cm-editor]:h-full [&_.cm-editor]:text-[13px]"
            ref={parent}
            // Before CodeMirror sees it, since its own Mod-Enter inserts a line.
            onKeyDownCapture={move |e: &Keyboard| {
                if let Some(submit) = &on_submit
                    && (e.meta_key() || e.ctrl_key())
                    && e.key() == "Enter"
                {
                    e.prevent_default();
                    e.stop_propagation();
                    submit();
                }
            }}
        />
    }
}
