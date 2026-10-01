// A `|` pattern's alternatives bind the same names (ADR 0124): where each
// binds one at the same place, as `Circle(r) | Sphere(r)` binds `s._0`,
// it's that place; where they don't, as `(0, x) | (x, 0)`, it's the place
// of the alternative that matched.

#[derive(Debug, Clone, PartialEq)]
enum Shape {
    Circle(f64),
    Sphere(f64),
    Square(f64),
    Rect { w: f64, h: f64 },
    Line { w: f64 },
}

fn size(s: &Shape) -> f64 {
    match s {
        Shape::Circle(r) | Shape::Sphere(r) => r * 2.0,
        Shape::Rect { w, .. } | Shape::Line { w } => *w,
        Shape::Square(x) => *x,
    }
}

// Bound at another place in each alternative.
fn pick(p: (i32, i32)) -> i32 {
    match p {
        (0, x) | (x, 0) => x,
        (a, b) => a * b,
    }
}

fn first_some(o: (Option<u8>, Option<u8>)) -> Option<u8> {
    match o {
        (Some(x), _) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

enum Token {
    Num(i64),
    Neg(i64),
    Word(String),
    Quoted(String),
}

fn text(t: &Token) -> String {
    match t {
        Token::Word(s) | Token::Quoted(s) => s.clone(),
        Token::Num(n) | Token::Neg(n) => n.to_string(),
    }
}

fn main() {
    let shapes = vec![
        Shape::Circle(1.5),
        Shape::Sphere(2.0),
        Shape::Square(3.0),
        Shape::Rect { w: 4.0, h: 1.0 },
        Shape::Line { w: 5.0 },
    ];
    for s in &shapes {
        println!("{}", size(s));
    }
    println!("{} {} {}", pick((0, 7)), pick((8, 0)), pick((2, 3)));
    println!("{:?} {:?} {:?}", first_some((Some(1), Some(2))), first_some((None, Some(2))), first_some((None, None)));
    for t in [Token::Num(4), Token::Neg(-5), Token::Word("w".into()), Token::Quoted("q".into())] {
        println!("{}", text(&t));
    }

    // Nested, with a guard that reads what's bound.
    for o in [Some(Shape::Circle(0.5)), Some(Shape::Sphere(3.0)), None] {
        match o {
            Some(Shape::Circle(r) | Shape::Sphere(r)) if r > 1.0 => println!("big {}", r),
            Some(Shape::Circle(r) | Shape::Sphere(r)) => println!("small {}", r),
            _ => println!("none"),
        }
    }

    // In an `if let`, and a `let` that can't fail.
    if let Shape::Rect { w, .. } | Shape::Line { w } = &shapes[4] {
        println!("width {}", w);
    }
    let r: Result<u32, u32> = Err(9);
    let (Ok(n) | Err(n)) = r;
    println!("{}", n);

    // Through a `&mut`, where it's the same place in each: written there.
    let mut grown = shapes.clone();
    for s in grown.iter_mut() {
        if let Shape::Circle(r) | Shape::Sphere(r) = s {
            *r *= 10.0;
        }
    }
    println!("{:?}", grown);
}
