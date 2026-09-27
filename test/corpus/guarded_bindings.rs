// A guard reads what its arm's pattern binds, before the arm's body runs:
// on a variable that can change, with a binding the body changes, and with
// the arm after it taking what the guard turned down.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Dot,
    Line(i32),
    Rect { w: u8, h: u8 },
}

fn describe(mut shape: Shape) -> String {
    let text = match shape {
        Shape::Line(n) if n > 10 => format!("long {n}"),
        Shape::Line(mut n) if n < 0 => {
            n = -n;
            format!("backwards {n}")
        }
        Shape::Line(n) => format!("line {n}"),
        Shape::Rect { w, h } if w == h => format!("square {w}"),
        Shape::Rect { w, h } => format!("rect {w}x{h}"),
        Shape::Dot => "dot".to_string(),
    };
    shape = Shape::Dot;
    format!("{text} then {shape:?}")
}

fn main() {
    for shape in [Shape::Line(20), Shape::Line(-3), Shape::Line(4), Shape::Rect { w: 2, h: 2 }, Shape::Rect { w: 2, h: 5 }, Shape::Dot] {
        println!("{}", describe(shape));
    }
    let mut total = 0;
    for value in [Some(3), None, Some(-1), Some(8)] {
        let mut current = value;
        total += match current {
            Some(n) if n > 5 => n * 10,
            Some(n) if n < 0 => {
                current = None;
                n
            }
            Some(n) => n,
            None => 0,
        };
        println!("{current:?} {total}");
    }
}
