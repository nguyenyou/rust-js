// A `&mut` a variable holds names its place (ADR 0099): `*y = 5` is a
// write to `x`, for a number, a `String`, an `Option` and a field. Passed
// on, it's the place a call is given, boxed. An index is fixed where the
// `&mut` is made, and so is an object reached through a reference that's
// assigned again. From rustc's tests, which stop at a `&mut` in a
// variable in 37 of them.
struct Point {
    x: i32,
    y: i32,
}

struct Node {
    count: i32,
}

struct Holder<'a> {
    list: &'a mut Vec<i32>,
    node: &'a mut Node,
}

fn bump(n: &mut i32) {
    *n += 1;
}

fn main() {
    let mut x = 3;
    let y = &mut x;
    *y = 5;
    *y += 2;
    println!("{}", *y);
    bump(y);
    bump(&mut *y);
    println!("{x}");

    let mut v = vec![1, 2, 3];
    let mut i = 0;
    let r = &mut v[i];
    i = 2;
    *r += 10;
    println!("{:?} {i}", v);

    let mut name = String::from("ada");
    let s = &mut name;
    s.push_str(" lovelace");
    *s = s.to_uppercase();
    println!("{name}");

    let mut o: Option<u32> = None;
    let p = &mut o;
    *p = Some(4);
    if let Some(n) = p {
        *n += 1;
    }
    println!("{o:?}");

    let mut pt = Point { x: 1, y: 2 };
    let px = &mut pt.x;
    *px *= 10;
    println!("{} {}", pt.x, pt.y);

    // Through a reference that's assigned again: the object of that moment.
    let mut a = Node { count: 1 };
    let mut b = Node { count: 10 };
    let mut cur = &mut a;
    let c = &mut cur.count;
    cur = &mut b;
    *c += 1;
    cur.count += 5;
    println!("{} {}", a.count, b.count);
    let mut xs = vec![1, 2];
    let mut ys = vec![10, 20];
    let mut list = &mut xs;
    let first = &mut list[0];
    list = &mut ys;
    *first += 1;
    list[1] += 1;
    println!("{:?} {:?}", xs, ys);

    // Through a reference in a field that's assigned again: the same.
    let (mut xs, mut ys) = (vec![1, 2], vec![10, 20]);
    let (mut na, mut nb) = (Node { count: 1 }, Node { count: 10 });
    let mut h = Holder { list: &mut xs, node: &mut na };
    let first = &mut h.list[0];
    let count = &mut h.node.count;
    h.list = &mut ys;
    h.node = &mut nb;
    *first += 1;
    *count += 1;
    h.list[1] += 1;
    println!("{:?} {:?} {} {}", xs, ys, na.count, nb.count);

    // In an element, or behind another reference: the same.
    let (mut xs, mut ys) = (vec![1, 2], vec![10, 20]);
    let mut refs = [&mut xs];
    let first = &mut refs[0][0];
    refs[0] = &mut ys;
    *first += 1;
    println!("{:?} {:?}", xs, ys);
    let (mut xs, mut ys) = (vec![1, 2], vec![10, 20]);
    let mut list = &mut xs;
    let outer = &mut list;
    let first = &mut outer[0];
    *outer = &mut ys;
    *first += 1;
    println!("{:?} {:?}", xs, ys);

    // A field of an element, and an element of an element.
    let mut nodes = vec![Node { count: 1 }, Node { count: 2 }];
    let mut j = 1;
    let n = &mut nodes[j].count;
    j = 0;
    *n *= 7;
    println!("{} {} {j}", nodes[0].count, nodes[1].count);
    let mut grid = vec![vec![1, 2], vec![3, 4]];
    let (row, col) = (1, 0);
    let g = &mut grid[row][col];
    *g = 30;
    println!("{:?}", grid);

    let mut z = 1;
    let a = &mut z;
    let b = &mut *a;
    *b += 1;
    *a += 1;
    println!("{z}");
}
