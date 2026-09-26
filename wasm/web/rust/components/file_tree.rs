// A folder's entries, as rows of a file explorer. A folder's own entries are
// a `FileTree` inside it, one level deeper: the component is recursive.

use std::rc::Rc;

use react::html::{div, li, span, ul};
use react::{Element, Style, component, fragment};

use super::file_item::{FileItem, FileItemProps};
use crate::styles::ROW;
use crate::tree::{Entry, Tree, in_order};

pub struct FileTreeProps {
    pub tree: &'static Tree,
    pub depth: u32,
    /// The crate root's path: listed first.
    pub first: String,
    /// The open file's path.
    pub selected: String,
    pub on_open: Rc<dyn Fn(String)>,
    /// With this, every file but the root can be deleted.
    pub on_delete: Option<Rc<dyn Fn(String)>>,
}

pub fn FileTree(FileTreeProps { tree, depth, first, selected, on_open, on_delete }: FileTreeProps) -> Element {
    let rows: Vec<Element> = in_order(tree, &first)
        .into_iter()
        .map(|(name, entry)| match entry {
            Entry::Folder(children) => li().key(format!("{name}/")).class_name("flex items-center").children(
                div().class_name("w-full").children((
                    span()
                        .class_name(format!("block {ROW} text-muted"))
                        .style(Style::new().padding_left(8 + depth * 12))
                        .children(format!("{name}/")),
                    ul().children(component(FileTree, FileTreeProps {
                        tree: children,
                        depth: depth + 1,
                        first: first.clone(),
                        selected: selected.clone(),
                        on_open: on_open.clone(),
                        on_delete: on_delete.clone(),
                    })),
                )),
            ),
            Entry::File(path) => component(FileItem, FileItemProps {
                name: name.clone(),
                path: path.clone(),
                depth,
                open: *path == selected,
                root: *path == first,
                on_open: on_open.clone(),
                on_delete: on_delete.clone(),
            })
            .key(path.clone()),
        })
        .collect();
    fragment(rows)
}
