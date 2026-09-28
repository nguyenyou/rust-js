// Immutable statics are their module's constants (ADR 0096), holding what
// rustc computed: numbers, text, structs, enums, and what a `&` in one points
// to. A `Copy` one that's changed after it's read is copied as it's read.
#[derive(Clone, Copy, Debug)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug)]
enum Level {
    Low,
    High(u8),
}

static ORIGIN: Point = Point { x: 0, y: 0 };
static CORNER: Point = Point { x: 2, y: -3 };
static LIMIT: u64 = 1 << 40;
static GREETING: &str = "hi";
static NAMES: [&str; 3] = ["ann", "bo", "cy"];
static PRIMES: &[u32] = &[2, 3, 5, 7];
static EMPTY: &[u32] = &[];
static LEVELS: [Level; 2] = [Level::Low, Level::High(9)];
static MAYBE: Option<char> = Some('z');
static PAIR: (bool, f64) = (true, 2.5);
static BOXED: &Point = &Point { x: 3, y: 4 };
static COMPUTED: i32 = -(7 * 6) + ORIGIN.x;

mod shapes {
    pub static SIDES: [u8; 3] = [3, 4, 6];
    pub(crate) static NAME: &str = "shapes";
}

fn moved(dx: i32) -> Point {
    let mut p = ORIGIN;
    p.x += dx;
    p
}

fn counted() -> usize {
    static WORDS: [&str; 2] = ["one", "two"];
    WORDS.len()
}

fn main() {
    println!("{:?} {:?} {:?}", moved(5), ORIGIN, moved(-2));
    println!("{} {} {}", LIMIT, GREETING, NAMES.join(","));
    println!("{:?} {} {:?}", PRIMES, PRIMES.iter().sum::<u32>(), EMPTY);
    println!("{:?} {:?} {:?}", LEVELS, MAYBE, PAIR);
    println!("{} {} {}", BOXED.x + BOXED.y, COMPUTED, counted());
    println!("{:?} {:?} {}", CORNER, shapes::SIDES, shapes::NAME);
    for name in NAMES.iter() {
        print!("{} ", name);
    }
    println!();
}
