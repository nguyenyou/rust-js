//! Dependency traversal over opaque item identities. Callers choose roots and
//! retention policy; traversal knows nothing about Rust types or emitted code.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Include each root and everything it transitively depends on. Duplicate edges,
/// self-edges and cycles are valid. Isolated roots remain reachable.
pub(crate) fn reachable<T: Copy + Eq + Hash>(
    roots: impl IntoIterator<Item = T>,
    edges: impl IntoIterator<Item = (T, T)>,
) -> HashSet<T> {
    let mut adjacency: HashMap<T, Vec<T>> = HashMap::new();
    for (from, to) in edges {
        adjacency.entry(from).or_default().push(to);
    }
    let mut todo: Vec<T> = roots.into_iter().collect();
    let mut reached = HashSet::new();
    while let Some(id) = todo.pop() {
        if reached.insert(id)
            && let Some(targets) = adjacency.get(&id)
        {
            todo.extend(targets);
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::reachable;
    use std::collections::HashSet;

    #[test]
    fn cycles_duplicates_and_disconnected_components() {
        let edges = [(0, 1), (0, 1), (1, 2), (2, 0), (2, 2), (3, 4), (4, 3)];
        assert_eq!(reachable([0, 0, 5], edges), HashSet::from([0, 1, 2, 5]));
        assert!(reachable([], edges).is_empty());
        assert_eq!(reachable([3], edges), HashSet::from([3, 4]));
    }

    #[test]
    fn deep_chains_do_not_use_the_call_stack() {
        let reached = reachable([0], (0..100_000).map(|n| (n, n + 1)));
        assert_eq!(reached.len(), 100_001);
        assert!(reached.contains(&100_000));
    }
}
