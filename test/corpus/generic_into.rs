// `x.into()` of a `T: Into<U>` is its dictionary's (ADR 0108): std's own
// conversion, which is often the value itself, or the crate's `From`.
struct Name(String);

impl From<&str> for Name {
    fn from(s: &str) -> Name {
        Name(s.to_uppercase())
    }
}

struct Celsius(f64);
struct Fahrenheit(f64);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Fahrenheit {
        Fahrenheit(c.0 * 9.0 / 5.0 + 32.0)
    }
}

// The idiom: a function that takes anything that makes a `String`.
fn greet(name: impl Into<String>) -> String {
    let name: String = name.into();
    format!("hello, {name}")
}

fn named<N: Into<Name>>(n: N) -> String {
    n.into().0
}

fn widened<T: Into<i64>>(values: Vec<T>) -> i64 {
    values.into_iter().map(|v| v.into()).sum()
}

fn fahrenheit<T: Into<Fahrenheit>>(t: T) -> f64 {
    t.into().0
}

fn main() {
    println!("{} | {} | {}", greet("ada"), greet(String::from("grace")), greet('x'));
    println!("{} {}", named("lin"), named(Name("kept".to_string())));
    println!("{} {}", widened(vec![1u8, 2, 3]), widened(vec![-5i32, 7]));
    println!("{}", fahrenheit(Celsius(100.0)));
}
