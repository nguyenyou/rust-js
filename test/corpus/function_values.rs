// A constructor, a closure as a `fn`, and a number's method, each taken as
// a value where a closure would go (ADR 0125): `.map(Some)` is `.map((value)
// => value)`, and `.map(Shape::Circle)` makes what `Shape::Circle(r)` does.

#[derive(Debug, Clone, Copy, PartialEq)]
struct Meters(f64);

#[derive(Debug)]
struct Pair(i32, &'static str);

#[derive(Debug)]
enum Shape {
    Circle(f64),
    Rect(f64, f64),
}

fn apply(f: fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}

fn twice<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(f(x))
}

fn main() {
    // Constructors: of an `Option`, a `Result`, a tuple struct and variants.
    let found: Vec<Option<i32>> = vec![1, 2].into_iter().map(Some).collect();
    let checked: Vec<Result<i32, String>> = vec![3].into_iter().map(Ok).collect();
    let lengths: Vec<Meters> = vec![1.5, 2.5].into_iter().map(Meters).collect();
    let circles: Vec<Shape> = vec![1.0, 2.0].into_iter().map(Shape::Circle).collect();
    println!("{:?} {:?} {:?} {:?}", found, checked, lengths, circles);
    let pairs: Vec<Pair> = vec![1, 2].into_iter().zip(["a", "b"]).map(|(n, s)| Pair(n, s)).collect();
    println!("{:?}", pairs);

    // In a variable, and of two fields.
    let make = Shape::Rect;
    println!("{:?}", make(2.0, 3.0));
    let wrap = Meters;
    println!("{:?} {}", wrap(4.0), wrap(4.0) == Meters(4.0));
    let as_fn: fn(f64) -> Meters = Meters;
    println!("{:?}", as_fn(5.0));

    // A closure that captures nothing, as a `fn`.
    let add_one: fn(i32) -> i32 = |x| x + 1;
    println!("{} {} {}", add_one(1), apply(add_one, 10), apply(|x| x * 3, 7));
    println!("{}", twice(add_one, 0));

    // A number's method.
    let sizes: Vec<i32> = vec![-3, 4, -5].into_iter().map(i32::abs).collect();
    let roots: Vec<f64> = vec![4.0, 9.0].into_iter().map(f64::sqrt).collect();
    println!("{:?} {:?}", sizes, roots);
    // An `f32`'s, rounded to one as its call is: squared, it isn't 2.
    let single: Vec<f32> = vec![2.0f32].into_iter().map(f32::sqrt).collect();
    println!("{} {}", single[0] * single[0], single[0] * single[0] == 2.0);
}
