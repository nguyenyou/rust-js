// A file in an explorer: its name opens it, and × deletes it, shown while
// the row is hovered. The crate root can't be deleted; it says "root".

use std::rc::Rc;

use react::{Element, Style};

use crate::styles::ROW;

pub struct FileItemProps {
    pub name: String,
    pub path: String,
    pub depth: u32,
    pub open: bool,
    pub root: bool,
    pub on_open: Rc<dyn Fn(String)>,
    pub on_delete: Option<Rc<dyn Fn(String)>>,
}

pub fn FileItem(
    FileItemProps {
        name,
        path,
        depth,
        open,
        root,
        on_open,
        on_delete,
    }: FileItemProps,
) -> Element {
    let opened = path.clone();
    let end = match on_delete {
        Some(_) if root => Some(jsx! { <span className="text-[11px] text-muted">{"root "}</span> }),
        Some(delete) => Some(jsx! {
            <button
                className="invisible cursor-pointer px-1.5 text-muted group-hover:visible focus:visible"
                aria-label={format!("Delete {path}")}
                onClick={move |_| delete(path.clone())}
            >
                {"×"}
            </button>
        }),
        None => None,
    };
    // `group`: its delete button shows while the row is hovered.
    jsx! {
        <li className="group flex items-center">
            <button
                className={format!("min-w-0 flex-1 cursor-pointer truncate {ROW} text-left aria-[current=true]:bg-selected")}
                style={Style::new().padding_left(8 + depth * 12)}
                aria-current={open}
                onClick={move |_| on_open(opened.clone())}
            >
                {name}
            </button>
            {end}
        </li>
    }
}
