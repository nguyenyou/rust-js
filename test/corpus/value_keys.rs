// A map or a set keyed by a struct, a tuple, an enum with fields or an
// `Option` finds its key by value, as a derived `Eq` compares it (ADR 0121).
// A `HashMap` with more than one entry goes in no order Rust promises, so
// what's printed is what's found, and maps of one.
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Shape {
    Dot,
    Circle(u32),
    Rect { w: u32, h: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Account {
    owner: String,
    id: u64,
    at: Point,
}

fn main() {
    // A tuple key: a grid's cells.
    let mut seen: HashSet<(i32, i32)> = HashSet::new();
    println!("{} {} {}", seen.insert((1, 2)), seen.insert((1, 2)), seen.insert((2, 1)));
    println!("{} {} {}", seen.len(), seen.contains(&(1, 2)), seen.contains(&(3, 3)));
    seen.remove(&(1, 2));
    println!("{} {}", seen.len(), seen.contains(&(1, 2)));

    // A struct key, its fields given in another order than declared.
    let mut walls: HashMap<Point, &str> = HashMap::new();
    walls.insert(Point { x: 1, y: 2 }, "stone");
    walls.insert(Point { y: 2, x: 1 }, "brick");
    println!("{} {:?} {:?}", walls.len(), walls.get(&Point { x: 1, y: 2 }), walls.get(&Point { x: 2, y: 1 }));
    println!("{} {}", walls[&Point { x: 1, y: 2 }], walls.contains_key(&Point { x: 1, y: 2 }));
    println!("{:?}", walls);

    // Counting by key, through an entry.
    let mut counts: HashMap<Shape, u32> = HashMap::new();
    for shape in [Shape::Circle(2), Shape::Dot, Shape::Circle(2), Shape::Rect { w: 1, h: 3 }, Shape::Circle(3)] {
        *counts.entry(shape).or_insert(0) += 1;
    }
    println!(
        "{} {:?} {:?} {:?} {:?}",
        counts.len(),
        counts.get(&Shape::Circle(2)),
        counts.get(&Shape::Dot),
        counts.get(&Shape::Rect { w: 1, h: 3 }),
        counts.get(&Shape::Rect { w: 3, h: 1 })
    );

    // An `Option` key, `None` among them.
    let mut by_parent: HashMap<Option<u32>, Vec<&str>> = HashMap::new();
    by_parent.entry(None).or_default().push("root");
    by_parent.entry(Some(1)).or_default().push("leaf");
    by_parent.entry(None).or_default().push("other root");
    println!("{:?} {:?} {:?}", by_parent.get(&None), by_parent.get(&Some(1)), by_parent.get(&Some(2)));

    // A key with a string, a BigInt and a struct in it.
    let ann = Account { owner: "ann".to_string(), id: 1 << 40, at: Point { x: 0, y: 0 } };
    let mut balances = HashMap::from([(ann.clone(), 10)]);
    *balances.get_mut(&ann).unwrap() += 5;
    let other = Account { id: 1 << 41, ..ann.clone() };
    println!("{:?} {:?}", balances.get(&ann), balances.get(&other));

    // A `Copy` key changed after it's put in: the map's is its own.
    let mut p = Point { x: 5, y: 5 };
    let mut marks: HashSet<Point> = HashSet::new();
    marks.insert(p);
    p.x = 6;
    println!("{} {} {:?}", marks.contains(&Point { x: 5, y: 5 }), marks.contains(&p), marks);

    // A clone's keys are its own: the original's, taken and changed, aren't.
    let copy = walls.clone();
    for (mut key, kind) in walls {
        key.x += 100;
        println!("{:?} {}", key, kind);
    }
    println!("{:?} {:?}", copy, copy.get(&Point { x: 1, y: 2 }));
    // A key that isn't `Copy` is moved out, not copied: the clone's is its own.
    let kept = balances.clone();
    for (mut account, balance) in balances {
        account.owner.push_str(" smith");
        println!("{} {}", account.owner, balance);
    }
    println!("{:?}", kept);

    let pairs: HashMap<(char, bool), u8> = vec![(('a', true), 1), (('a', false), 2), (('a', true), 3)].into_iter().collect();
    println!("{} {:?} {:?}", pairs.len(), pairs.get(&('a', true)), pairs.get(&('a', false)));
}
