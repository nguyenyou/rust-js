// Traits with default methods and a blanket impl, a derived `Ord` sorted,
// `..Default::default()`, a generic `next()?`, `scan`, `drain` and
// `split_off`, `total_cmp`, `char::from_digit`, and `{:e}` (ADR 0076).

use std::cell::Cell;
use std::fmt::Display;

pub trait Describe {
    fn name(&self) -> String;
    fn describe(&self) -> String {
        format!("<{}>", self.name())
    }
    fn shout(&self) -> String {
        self.describe().to_uppercase()
    }
}

pub struct Dog;
impl Describe for Dog {
    fn name(&self) -> String {
        "dog".into()
    }
}
pub struct Cat {
    pub lives: u8,
}
impl Describe for Cat {
    fn name(&self) -> String {
        format!("cat{}", self.lives)
    }
    fn describe(&self) -> String {
        format!("[{}]", self.name())
    }
}

pub trait Labeled {
    fn label(&self) -> String;
}
impl<T: Display> Labeled for T {
    fn label(&self) -> String {
        format!("#{self}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub name: String,
    pub retries: u32,
    pub verbose: bool,
    pub ratio: f64,
}

fn largest<T: PartialOrd + Copy>(items: &[T]) -> Option<T> {
    let mut it = items.iter();
    let mut best = *it.next()?;
    for &x in it {
        if x > best {
            best = x;
        }
    }
    Some(best)
}

pub fn tour() -> String {
    let mut out = String::new();
    let pets: Vec<Box<dyn Describe>> = vec![Box::new(Dog), Box::new(Cat { lives: 9 })];
    for p in &pets {
        out += &format!("{} {} {}\n", p.name(), p.describe(), p.shout());
    }
    out += &format!("{} {} {}\n", 42.label(), "hi".label(), 2.5.label());
    let mut vs = vec![
        Version {
            major: 1,
            minor: 2,
            patch: 3,
        },
        Version {
            major: 1,
            minor: 0,
            patch: 9,
        },
        Version::default(),
        Version {
            major: 0,
            minor: 9,
            ..Default::default()
        },
    ];
    vs.sort();
    out += &format!(
        "{:?}\n",
        vs.iter()
            .map(|v| format!("{}.{}.{}", v.major, v.minor, v.patch))
            .collect::<Vec<_>>()
    );
    out += &format!("{:?} {}\n", vs.iter().max(), vs[1] < vs[2]);
    let c = Config {
        name: "svc".into(),
        retries: 3,
        ..Default::default()
    };
    out += &format!("{:?}\n", c);
    out += &format!(
        "{:?} {:?} {:?}\n",
        largest(&[3, 9, 2]),
        largest(&[1.5, -2.0]),
        largest::<u8>(&[])
    );
    let running: Vec<u32> = [1, 2, 3, 4]
        .iter()
        .scan(0, |acc, &x| {
            *acc += x;
            Some(*acc)
        })
        .collect();
    let mut v: Vec<u32> = (1..=8).collect();
    let tail = v.split_off(5);
    let drained: Vec<u32> = v.drain(1..3).collect();
    out += &format!("{running:?} {v:?} {tail:?} {drained:?}\n");
    let mut fs: Vec<f64> = vec![2.5, -1.0, 3.25, 0.0];
    fs.sort_by(|a, b| a.total_cmp(b));
    let total: f64 = fs.iter().sum();
    out += &format!(
        "{fs:?} {total} {:e} {:08.3} {:?}\n",
        1234.5,
        -3.14159,
        std::char::from_digit(7, 10)
    );
    let words = vec!["a".to_string(), "b".to_string()];
    out += &format!("{} {}\n", words.join("-"), words.concat());
    out
}

fn tick(c: &Cell<u32>, tag: u32) -> u32 {
    c.set(c.get() * 10 + tag);
    tag
}
fn made(c: &Cell<u32>) -> Version {
    tick(c, 9);
    Version {
        major: 7,
        minor: 7,
        patch: 7,
    }
}

pub fn edges() -> String {
    let order = Cell::new(0);
    // Rust works out the fields, then the base.
    let v = Version {
        patch: tick(&order, 1),
        major: tick(&order, 2),
        ..made(&order)
    };
    let upto: Vec<u32> = [5, 3, 8, 1]
        .iter()
        .scan(1, |acc, &x| {
            *acc *= x;
            if *acc > 100 { None } else { Some(*acc) }
        })
        .collect();
    let mut fs = vec![f64::NAN, 1.0, -0.0, 0.0, f64::NEG_INFINITY, -1.0, f64::INFINITY];
    fs.sort_by(|a, b| a.total_cmp(b));
    let digits: Vec<Option<char>> = [(0, 1), (9, 10), (10, 10), (15, 16), (35, 36)]
        .iter()
        .map(|&(n, r)| std::char::from_digit(n, r))
        .collect();
    let points: Vec<Option<char>> = [0x41, 0xe9, 0xd800, 0x1f600, 0x110000]
        .iter()
        .map(|&n| char::from_u32(n))
        .collect();
    format!(
        "{v:?} {} {upto:?}\n{fs:?}\n{digits:?} {points:?}\n{:e} {:e} {:e} {:E} {:e} {:e}",
        order.get(),
        1234u32,
        0.0,
        -0.00025,
        6.02e23,
        f64::NAN,
        100i32
    )
}

pub fn panics(i: u32) -> Vec<u32> {
    let mut v = vec![1, 2, 3];
    match i {
        0 => v.split_off(4),
        1 => v.drain(2..1).collect(),
        _ => v.drain(1..5).collect(),
    }
}

pub fn report() -> String {
    format!("{}{}\n", tour(), edges())
}
