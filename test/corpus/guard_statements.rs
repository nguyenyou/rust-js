// A guard that needs statements of its own, a `match` or a block that
// writes, runs after its arm's pattern is tested; one that fails goes on to
// the later arms.
#[derive(Clone, Copy)]
enum Shape {
    Dot,
    Square(i32),
    Rect { w: i32, h: i32 },
}

fn size(s: Shape) -> i32 {
    match s {
        Shape::Dot => 0,
        Shape::Square(n) => n * n,
        Shape::Rect { w, h } => w * h,
    }
}

fn describe(s: Shape, seen: &mut i32) -> &'static str {
    match s {
        Shape::Square(n) if match s { Shape::Square(_) => n > 2, _ => false } => "big square",
        Shape::Square(_) if { *seen += 1; *seen > 2 } => "square, seen often",
        Shape::Square(_) => "square",
        _ if size(s) > 10 => "big",
        Shape::Rect { w, .. } if { let half = w / 2; half > 0 } => return "wide rect",
        _ => "other",
    }
}

fn main() {
    let mut seen = 0;
    let shapes = [
        Shape::Square(3),
        Shape::Square(1),
        Shape::Dot,
        Shape::Square(2),
        Shape::Rect { w: 5, h: 3 },
        Shape::Rect { w: 3, h: 1 },
        Shape::Rect { w: 1, h: 1 },
        Shape::Square(1),
    ];
    for s in shapes {
        println!("{}", describe(s, &mut seen));
    }
    let mut odd = 0;
    for i in 0..6 {
        match i {
            n if { odd += n % 2; n % 3 == 0 } => continue,
            n => println!("{n} {odd}"),
        }
    }
    let mut tries = 0;
    for s in shapes {
        let label = match s {
            Shape::Rect { w, h } if { tries += 1; w == h } => "square rect",
            Shape::Rect { .. } => "rect",
            _ => "not a rect",
        };
        println!("{label} {tries}");
    }
}
