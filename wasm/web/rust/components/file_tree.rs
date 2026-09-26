// A folder's entries, as rows of a file explorer. A folder's own entries are
// a `FileTree` inside it, one level deeper: the component is recursive.

use std::rc::Rc;

use react::{Element, Style};

use super::file_item::FileItem;
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

pub fn FileTree(
    FileTreeProps {
        tree,
        depth,
        first,
        selected,
        on_open,
        on_delete,
    }: FileTreeProps,
) -> Element {
    let rows: Vec<Element> = in_order(tree, &first)
        .into_iter()
        .map(|(name, entry)| match entry {
            Entry::Folder(children) => jsx! {
                <li key={format!("{name}/")} className="flex items-center">
                    <div className="w-full">
                        <span className={format!("block {ROW} text-muted")} style={Style::new().padding_left(8 + depth * 12)}>
                            {format!("{name}/")}
                        </span>
                        <ul>
                            <FileTree
                                tree={children}
                                depth={depth + 1}
                                first={first.clone()}
                                selected={selected.clone()}
                                onOpen={on_open.clone()}
                                onDelete={on_delete.clone()}
                            />
                        </ul>
                    </div>
                </li>
            },
            Entry::File(path) => jsx! {
                <FileItem
                    name={name.clone()}
                    path={path.clone()}
                    depth={depth}
                    open={*path == selected}
                    root={*path == first}
                    onOpen={on_open.clone()}
                    onDelete={on_delete.clone()}
                    key={path.clone()}
                />
            },
        })
        .collect();
    jsx! { <>{rows}</> }
}
