// A store that applies events and checks rules: what a program does with
// `let ... else`, range patterns and `@`, `bool::then`, a `&mut` to a number
// in a map, closures that return closures, and `Rc<RefCell<..>>` (ADR 0067).

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Restock { item: String, qty: u32 },
    Sale { item: String, qty: u32, price: f64 },
    Audit,
}

#[derive(Debug, Default)]
pub struct Store {
    stock: HashMap<String, u32>,
    revenue: f64,
    log: Vec<String>,
}

pub trait Rule {
    fn name(&self) -> String;
    fn check(&self, store: &Store) -> Option<String>;
}

struct MinStock {
    item: String,
    min: u32,
}
impl Rule for MinStock {
    fn name(&self) -> String {
        format!("min-stock({})", self.item)
    }
    fn check(&self, store: &Store) -> Option<String> {
        let have = store.stock.get(&self.item).copied().unwrap_or(0);
        if have < self.min {
            Some(format!("{} low: {have} < {}", self.item, self.min))
        } else {
            None
        }
    }
}
struct Revenue(f64);
impl Rule for Revenue {
    fn name(&self) -> String {
        "revenue".to_string()
    }
    fn check(&self, store: &Store) -> Option<String> {
        (store.revenue < self.0).then(|| format!("revenue {:.2} under {:.2}", store.revenue, self.0))
    }
}

impl fmt::Display for Store {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut items: Vec<_> = self.stock.iter().collect();
        items.sort();
        for (name, qty) in items {
            write!(f, "{name}={qty} ")?;
        }
        write!(f, "| {:.2}", self.revenue)
    }
}

impl Store {
    pub fn apply(&mut self, e: &Event) -> Result<(), String> {
        match e {
            Event::Restock { item, qty } => {
                *self.stock.entry(item.clone()).or_insert(0) += qty;
            }
            Event::Sale { item, qty, price } => {
                let Some(have) = self.stock.get_mut(item) else {
                    return Err(format!("unknown item {item}"));
                };
                if *have < *qty {
                    return Err(format!("not enough {item}: {have} < {qty}"));
                }
                *have -= qty;
                self.revenue += *qty as f64 * price;
            }
            Event::Audit => self.log.push(format!("audit at {:.2}", self.revenue)),
        }
        Ok(())
    }
}

fn discount(pct: f64) -> impl Fn(f64) -> f64 {
    move |p| p * (1.0 - pct / 100.0)
}

fn classify(n: i32) -> &'static str {
    match n {
        i32::MIN..=-1 => "negative",
        0 => "zero",
        x @ 1..=9 if x % 2 == 0 => "small even",
        1..=9 => "small odd",
        _ => "large",
    }
}

pub fn report() -> String {
    let mut store = Store::default();
    let events = vec![
        Event::Restock {
            item: "apple".into(),
            qty: 10,
        },
        Event::Restock {
            item: "pear".into(),
            qty: 3,
        },
        Event::Sale {
            item: "apple".into(),
            qty: 4,
            price: 0.5,
        },
        Event::Sale {
            item: "kiwi".into(),
            qty: 1,
            price: 2.0,
        },
        Event::Sale {
            item: "pear".into(),
            qty: 5,
            price: 1.0,
        },
        Event::Audit,
    ];
    let mut out = String::new();
    for e in &events {
        if let Err(msg) = store.apply(e) {
            out += &format!("error: {msg}\n");
        }
    }
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MinStock {
            item: "pear".into(),
            min: 5,
        }),
        Box::new(Revenue(10.0)),
    ];
    for r in &rules {
        match r.check(&store) {
            Some(w) => out += &format!("[{}] {w}\n", r.name()),
            None => out += &format!("[{}] ok\n", r.name()),
        }
    }
    out += &format!("{store}\n{:?}\n", store.log);
    let half = discount(50.0);
    let mut prices = vec![3.5, 1.25, 9.0, 0.75];
    prices.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let cheap: Vec<f64> = prices.iter().map(|&p| half(p)).collect();
    out += &format!(
        "{prices:?} {cheap:?} {}\n",
        events.iter().filter(|e| matches!(e, Event::Sale { .. })).count()
    );
    let counter = Rc::new(RefCell::new(0));
    let bump = {
        let c = Rc::clone(&counter);
        move |n: i32| *c.borrow_mut() += n
    };
    for n in [-3, 0, 4, 7, 12] {
        bump(n);
        out += &format!("{} ", classify(n));
    }
    out += &format!("total={} again={}\n", counter.borrow(), counter.borrow());
    let mut stack = vec![1, 2, 3];
    let mut popped = vec![];
    while let Some(top) = stack.pop() {
        if top == 2 {
            stack.push(10);
        }
        popped.push(top);
    }
    out += &format!("{popped:?}\n");
    out += &format!(
        "{:?} {:?}\n",
        tallies(&["a", "b", "a", "b", "c"]),
        bands(&[0, 3, 42, 150, 250])
    );
    out
}

// A `&mut` to a map's number, from `get_mut`, and the count idiom with a `&u32`.
pub fn tallies(words: &[&str]) -> Vec<(String, u32)> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    let one = 1;
    for w in words {
        *counts.entry(w.to_string()).or_insert(0) += &one;
        if let Some(n) = counts.get_mut("b") {
            *n *= 2;
        }
    }
    let mut all: Vec<(String, u32)> = counts.into_iter().collect();
    all.sort();
    all
}

pub fn bands(xs: &[u8]) -> Vec<String> {
    xs.iter()
        .map(|&x| match x {
            0 => "none".to_string(),
            n @ 1..10 => format!("few {n}"),
            n @ (10..=99 | 200..) => format!("many {n}"),
            _ => "odd".into(),
        })
        .collect()
}
