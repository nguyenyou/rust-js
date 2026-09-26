// Enums with fields (ADR 0033), in ReScript's shapes: a variant without
// fields is its name, `"Empty"`, and one with fields is an object tagged
// with its name, `{ TAG: "Circle", _0: 2 }` or `{ TAG: "Rect", w: 2, h: 3 }`.

#[derive(Clone, Copy, PartialEq)]
pub enum Shape {
    Empty,
    Circle(i32),
    Rect { w: i32, h: i32 },
}

pub fn circle(r: i32) -> Shape {
    Shape::Circle(r)
}

pub fn rect(w: i32, h: i32) -> Shape {
    Shape::Rect { w, h }
}

pub fn empty() -> Shape {
    Shape::Empty
}

pub fn area(s: Shape) -> i32 {
    match s {
        Shape::Empty => 0,
        Shape::Circle(r) => 3 * r * r,
        Shape::Rect { w, h } => w * h,
    }
}

/// Patterns inside variants, guards, `|`, and `..`.
pub fn classify(s: Shape) -> u32 {
    match s {
        Shape::Circle(r) if r > 10 => 3,
        Shape::Circle(_) => 2,
        Shape::Rect { w: 0, .. } | Shape::Empty => 0,
        Shape::Rect { .. } => 1,
    }
}

pub fn is_round(s: Shape) -> bool {
    matches!(s, Shape::Circle(_))
}

pub fn width(s: Shape) -> i32 {
    if let Shape::Rect { w, .. } = s { w } else { -1 }
}

/// `==` compares variant and fields.
pub fn same(a: Shape, b: Shape) -> bool {
    a == b
}

/// A recursive enum, matched through a reference.
pub enum Tree {
    Leaf(i32),
    Node(Box<Tree>, Box<Tree>),
}

fn build(depth: u32) -> Tree {
    if depth == 0 {
        Tree::Leaf(1)
    } else {
        Tree::Node(Box::new(build(depth - 1)), Box::new(Tree::Leaf(depth as i32)))
    }
}

fn sum(t: &Tree) -> i32 {
    match t {
        Tree::Leaf(n) => *n,
        Tree::Node(left, right) => sum(left) + sum(right),
    }
}

pub fn tree_sum(depth: u32) -> i32 {
    sum(&build(depth))
}

/// `Result` is an enum like any other: `{ TAG: "Ok", _0: v }`, as in ReScript.
pub fn checked_div(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 { Err(String::from("divide by zero")) } else { Ok(a / b) }
}

pub fn div_or(a: i32, b: i32, fallback: i32) -> i32 {
    match checked_div(a, b) {
        Ok(q) => q,
        Err(_) => fallback,
    }
}
