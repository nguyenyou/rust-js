//! A type inside itself, cloned by a derive the library wrote.
#[derive(Debug, Clone, PartialEq)]
pub enum Tree {
    Leaf(u32),
    Node(Box<Tree>, Box<Tree>),
}

impl Tree {
    pub fn grow(&mut self) {
        match self {
            Tree::Leaf(n) => *n += 1,
            Tree::Node(a, b) => {
                a.grow();
                b.grow();
            }
        }
    }
}

pub fn sample() -> Tree {
    Tree::Node(Box::new(Tree::Leaf(1)), Box::new(Tree::Node(Box::new(Tree::Leaf(2)), Box::new(Tree::Leaf(3)))))
}
