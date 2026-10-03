// A trait object of std's `Display` or `Error` is a value and its
// dictionary, `{ value, impl }`, as one of the crate's own traits is: the
// dictionary generic code is given for the type. It's shown when it's
// shown, by its `fmt`.
use std::cell::Cell;
use std::error::Error;
use std::fmt;

struct Point {
    x: i32,
    y: i32,
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

struct Counter(Cell<u32>);

impl fmt::Display for Counter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "counted {}", self.0.get())
    }
}

#[derive(Debug)]
struct Missing(String);

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "missing {}", self.0)
    }
}

impl Error for Missing {}

fn find(name: &str) -> Result<u32, Missing> {
    match name {
        "one" => Ok(1),
        _ => Err(Missing(name.to_string())),
    }
}

fn lookup(name: &str) -> Result<u32, Box<dyn Error>> {
    let n = find(name)?;
    if n > 10 {
        return Err("too big".into());
    }
    Ok(n)
}

fn describe(d: &dyn fmt::Display) -> String {
    format!("<{}>", d)
}

fn main() {
    let byte: &dyn fmt::Display = &33u8;
    let boxed: Box<dyn fmt::Display> = Box::new(Point { x: 1, y: 2 });
    println!("{} {} {}", byte, boxed, describe(&"text"));
    println!("{}", boxed.to_string());

    let all: Vec<Box<dyn fmt::Display>> = vec![Box::new(1.5f64), Box::new(String::from("s")), Box::new(Point { x: 3, y: 4 })];
    for d in &all {
        print!("{d} ");
    }
    println!();

    let counter = Counter(Cell::new(1));
    let shown: &dyn fmt::Display = &counter;
    counter.0.set(2);
    println!("{shown}");

    match lookup("one") {
        Ok(n) => println!("found {n}"),
        Err(e) => println!("error {e}"),
    }
    match lookup("two") {
        Ok(n) => println!("found {n}"),
        Err(e) => println!("error {e} / {e:?} / {} / {}", e.to_string(), e.source().is_none()),
    }
    let from_string: Box<dyn Error> = From::from(String::from("made"));
    let from_str: Box<dyn Error + Send + Sync> = From::from("also made");
    println!("{from_string} {from_str:?}");
}
