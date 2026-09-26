// Structs and tuples: plain JS objects and arrays (ADR 0020).

/// A struct with named fields is an object: `{ x: 1, y: 2 }`.
#[derive(Clone, Copy)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A tuple struct is an array, like a tuple: `[3, 4]`.
#[derive(Clone, Copy)]
pub struct Size(pub u32, pub u32);

/// Not `Copy`, so assigning one moves it: it's never copied.
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

pub fn point(x: i32, y: i32) -> Point {
    Point { x, y }
}

pub fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect { origin: Point { x, y }, size: Size(w, h) }
}

/// Fields are read with `.`, and a tuple's with `[i]`.
pub fn area(r: Rect) -> u32 {
    r.size.0 * r.size.1
}

/// Changing a field in place: `p.x += dx`.
pub fn moved(x: i32, y: i32, dx: i32) -> Point {
    let mut p = point(x, y);
    p.x += dx;
    p
}

/// Nested fields, read and written.
pub fn grow(w: u32, h: u32, by: u32) -> Rect {
    let mut r = rect(0, 0, w, h);
    r.size.0 += by;
    r.size.1 = r.size.1 * 2;
    r.origin = point(-1, -1);
    r
}

/// `Point` is `Copy` and changed in place in this crate, so copying one
/// copies the JS object: changing `b` must leave `a` alone.
pub fn copies_are_separate(x: i32) -> (i32, i32) {
    let a = point(x, 0);
    let mut b = a;
    b.x += 1;
    (a.x, b.x)
}

/// A `mut` parameter is a copy too.
fn bump(mut p: Point) -> Point {
    p.y += 10;
    p
}

pub fn caller_keeps_its_point(y: i32) -> (Point, Point) {
    let a = point(0, y);
    let b = bump(a);
    (a, b)
}

/// `Rect` moves, but the `Point` inside it was copied from `a`.
pub fn moves_share_nothing(x: i32) -> (i32, i32) {
    let a = point(x, x);
    let r = Rect { origin: a, size: Size(1, 1) };
    let mut s = r;
    s.origin.x = 0;
    (a.x, s.origin.x)
}

/// A variable bound by a pattern keeps its value, even when the struct it
/// came from is then moved and changed.
pub fn bound_before_move(x: i32) -> (i32, i32) {
    let r = rect(x, x, 1, 1);
    let Rect { origin: Point { x: before, .. }, .. } = r;
    let mut s = r;
    s.origin.x = 0;
    (before, s.origin.x)
}

/// Returned through a reference, a `Copy` field is a copy: the caller's
/// struct still has its own.
fn origin_of(r: &Rect) -> Point {
    r.origin
}

pub fn returned_copy_is_separate(x: i32) -> (i32, i32) {
    let r = rect(x, x, 1, 1);
    let mut p = origin_of(&r);
    p.x += 1;
    (r.origin.x, p.x)
}

/// An `Option` of a `Copy` struct is copied like the struct.
pub struct Marker {
    pub at: Option<Point>,
}

fn marked(m: &Marker) -> Option<Point> {
    m.at
}

pub fn option_copy_is_separate(x: i32) -> (i32, i32) {
    let m = Marker { at: Some(point(x, 0)) };
    let mut p = marked(&m).unwrap_or(point(0, 0));
    p.x += 1;
    let kept = match m.at {
        Some(q) => q.x,
        None => 0,
    };
    (kept, p.x)
}

/// `*r` of a reference a call returns is a copy too.
pub fn deref_copy_is_separate(x: i32) -> (i32, i32) {
    let points = vec![point(x, 0)];
    let mut p = *points.first().unwrap();
    p.x += 1;
    (points.first().unwrap().x, p.x)
}

/// Struct update syntax: fields not written come from `base`.
pub fn with_x(x: i32, y: i32) -> Point {
    let base = point(0, y);
    Point { x, ..base }
}

/// Fields run in the order written, though the object lists them in
/// declaration order. Here `y` is written first, so its division by zero
/// panics before `x`'s remainder does.
pub fn written_order(a: i32, b: i32) -> Point {
    Point { y: 100 / a, x: 100 % b }
}

/// Tuples: built, returned and taken apart.
pub fn divmod(a: u32, b: u32) -> (u32, u32) {
    (a / b, a % b)
}

pub fn divmod_sum(a: u32, b: u32) -> u32 {
    let (q, r) = divmod(a, b);
    q + r
}

/// A pattern in a parameter, and `match` on a tuple of variables.
pub fn classify((a, b): (i32, i32)) -> u32 {
    match (a, b) {
        (0, 0) => 0,
        (0, _) | (_, 0) => 1,
        (x, y) if x == y => 2,
        (x, _) if x < 0 => 3,
        _ => 4,
    }
}

/// `match` on struct fields, with bindings.
pub fn quadrant(x: i32, y: i32) -> i32 {
    match point(x, y) {
        Point { x: 0, .. } | Point { y: 0, .. } => 0,
        Point { x, y } if x > 0 && y > 0 => 1,
        Point { x, .. } if x < 0 => 2 + (y < 0) as i32,
        Point { y, .. } => y * 4,
    }
}
