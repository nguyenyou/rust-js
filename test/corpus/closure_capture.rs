// Closures capture by reference, by mutable reference, or by move, and each
// sees what the others changed, as Rust's do.
fn make_counter() -> impl FnMut() -> i32 {
    let mut count = 0;
    move || {
        count += 1;
        count
    }
}

fn apply<F: FnMut(i32)>(mut f: F, items: &[i32]) {
    for &x in items {
        f(x);
    }
}

fn main() {
    let mut total = 0;
    apply(|x| total += x, &[1, 2, 3]);
    println!("total {total}");

    let mut a = make_counter();
    let mut b = make_counter();
    println!("{} {} {} {}", a(), a(), b(), a());

    let mut names = vec!["x".to_string()];
    {
        let mut add = |s: &str| names.push(s.to_string());
        add("y");
        add("z");
    }
    println!("{names:?}");

    let base = 10;
    let adders: Vec<Box<dyn Fn(i32) -> i32>> = (0..3).map(|i| Box::new(move |x| x + i * base) as Box<dyn Fn(i32) -> i32>).collect();
    println!("{:?}", adders.iter().map(|f| f(1)).collect::<Vec<_>>());

    let mut value = 5;
    let mut double = || value *= 2;
    double();
    double();
    println!("value {value}");

    let text = String::from("moved");
    let owns = move || text.chars().count();
    println!("len {}", owns());
}
