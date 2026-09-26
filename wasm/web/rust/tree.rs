// File paths as a tree, for the file explorers.

use std::cmp::Ordering;

/// A folder's entries, in the order they came: subfolders, or a file's full path.
pub type Tree = Vec<(String, Entry)>;

pub enum Entry {
    Folder(Tree),
    File(String),
}

pub fn build_tree(paths: &[String]) -> Tree {
    let mut tree: Tree = Vec::new();
    for path in paths {
        let mut folder = &mut tree;
        let name = match path.rsplit_once('/') {
            Some((folders, name)) => {
                for part in folders.split('/') {
                    if !folder.iter().any(|(n, _)| n == part) {
                        folder.push((part.to_string(), Entry::Folder(Vec::new())));
                    }
                    folder = match folder.iter_mut().find(|(n, _)| n == part) {
                        Some((_, Entry::Folder(children))) => children,
                        _ => unreachable!("a folder, found or just made"),
                    };
                }
                name
            }
            None => path.as_str(),
        };
        folder.push((name.to_string(), Entry::File(path.clone())));
    }
    tree
}

/// A folder's entries as the explorer shows them: `first` (the crate root),
/// then by name, a module's file just before its folder: `geometry.rs`, then
/// `geometry/` ("." sorts before "/").
pub fn in_order<'a>(tree: &'a Tree, first: &str) -> Vec<&'a (String, Entry)> {
    let key = |(name, entry): &(String, Entry)| match entry {
        Entry::Folder(_) => format!("{name}/"),
        Entry::File(_) => name.clone(),
    };
    let is_first = |entry: &Entry| matches!(entry, Entry::File(path) if path == first);
    let mut entries: Vec<&(String, Entry)> = tree.iter().collect();
    entries.sort_by(|a, b| {
        if is_first(&a.1) {
            Ordering::Less
        } else if is_first(&b.1) {
            Ordering::Greater
        } else {
            key(a).cmp(&key(b))
        }
    });
    entries
}
