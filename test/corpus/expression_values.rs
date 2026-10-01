// An `if let` is a value wherever a value goes (ADR 0130), a trait's
// function or std's is a function value as a closure is, and `Self` of a
// unit struct is the struct.

#[derive(Debug)]
enum Shape {
    Circle(f64),
    Square { side: f64 },
    Dot,
}

trait Area {
    fn area(&self) -> f64;
    fn unit() -> Self;
}

impl Area for Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle(r) => 3.0 * r * r,
            Shape::Square { side } => side * side,
            Shape::Dot => 0.0,
        }
    }

    fn unit() -> Self {
        Shape::Square { side: 1.0 }
    }
}

struct Marker;

impl Marker {
    fn new() -> Self {
        Self
    }

    fn is_marker(&self) -> bool {
        matches!(self, Self)
    }
}

struct Loud(u8);

impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn twice(n: i32) -> i32 {
    n * 2
}

fn find(items: &[i32], x: i32) -> Option<usize> {
    items.iter().position(|&y| y == x)
}

fn radius_plus_one(s: &Shape) -> f64 {
    1.0 + if let Shape::Circle(r) = s { *r } else { 0.0 }
}

fn give(f: fn(Loud), loud: Loud) {
    f(loud);
    println!("given");
}

// An `if let` that always matches is one Rust warns of, but takes.
#[allow(irrefutable_let_patterns)]
fn main() {
    let maybe = Some(3);
    println!("{}", twice(if let Some(x) = maybe { x } else { 0 }));
    let missing = !(if let Some(_) = maybe { true } else { false });
    assert!(if let Some(3) = maybe { true } else { false });
    let items = vec![if let Some(x) = maybe { x + 1 } else { 0 }, 7];
    let pair = (if let Some(i) = find(&items, 7) { i } else { 99 }, 2);
    let pairs = [Some((1, 2)), None];
    let sums: Vec<i32> = pairs.iter().map(|p| 10 + if let Some((a, b)) = p { a + b } else { 0 }).collect();
    let words = ["a", "bb"];
    let first = 1 + if let [first, ..] = words { first.chars().count() } else { 0 };
    println!("{} {:?} {:?} {:?} {}", missing, items, pair, sums, first);
    println!("{} {}", radius_plus_one(&Shape::Circle(2.0)), radius_plus_one(&Shape::Dot));

    let shapes = [Shape::Circle(1.0), Shape::Square { side: 2.0 }];
    let areas: Vec<f64> = shapes.iter().map(Area::area).collect();
    let make = <Shape as Area>::unit;
    let size = std::mem::size_of::<u16>;
    let is_some = Option::is_some;
    let dropped = vec![1, 2, 3].into_iter().map(drop).count();
    println!("{:?} {:?} {} {} {}", areas, make(), size(), is_some(&maybe), dropped);

    let quiet = drop;
    quiet(Loud(1));
    give(drop, Loud(2));
    give(std::mem::forget, Loud(3));
    println!("{}", Marker::new().is_marker());
}
