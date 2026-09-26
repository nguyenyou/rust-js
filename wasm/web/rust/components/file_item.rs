// A file in an explorer: its name opens it, and × deletes it, shown while
// the row is hovered. The crate root can't be deleted; it says "root".

use std::rc::Rc;

use react::html::{button, li, span};
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

pub fn FileItem(FileItemProps { name, path, depth, open, root, on_open, on_delete }: FileItemProps) -> Element {
    let opened = path.clone();
    let end = match on_delete {
        Some(_) if root => Some(span().class_name("text-[11px] text-muted").children("root ")),
        Some(delete) => Some(
            button()
                .class_name("invisible cursor-pointer px-1.5 text-muted group-hover:visible focus:visible")
                .attr("aria-label", format!("Delete {path}"))
                .on_click(move |_| delete(path.clone()))
                .children("×"),
        ),
        None => None,
    };
    // `group`: its delete button shows while the row is hovered.
    li().class_name("group flex items-center").children((
        button()
            .class_name(format!("min-w-0 flex-1 cursor-pointer truncate {ROW} text-left aria-[current=true]:bg-selected"))
            .style(Style::new().padding_left(8 + depth * 12))
            .attr("aria-current", open)
            .on_click(move |_| on_open(opened.clone()))
            .children(name),
        end,
    ))
}
