// An associated type, `type Item` (ADR 0106), is a type only a caller knows
// in generic code, as a type parameter is: `drain`'s `Vec<S::Item>` is a
// `Vec<u32>` of one caller's and a `Vec<String>` of another's. std's are the
// same: `I::Item` of an `Iterator`.
use std::fmt::{Debug, Display};

trait Source {
    type Item;
    fn next_item(&mut self) -> Option<Self::Item>;
}

struct Count {
    n: u32,
}

impl Source for Count {
    type Item = u32;
    fn next_item(&mut self) -> Option<u32> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;
            Some(self.n)
        }
    }
}

struct Words {
    list: Vec<String>,
}

impl Source for Words {
    type Item = String;
    fn next_item(&mut self) -> Option<Self::Item> {
        self.list.pop()
    }
}

fn drain<S: Source>(s: &mut S) -> Vec<S::Item> {
    let mut out = Vec::new();
    while let Some(x) = s.next_item() {
        out.push(x);
    }
    out
}

// An equality bound: `S::Item` is `u32` here.
fn total<S: Source<Item = u32>>(s: &mut S) -> u32 {
    drain(s).iter().sum()
}

// A bound on the associated type, in a `where`: its dictionary is given, as
// a type parameter's is, `SItemDebug`.
fn shown<S: Source>(s: &mut S) -> String
where
    S::Item: Debug,
{
    format!("{:?}", drain(s))
}

// A bound the trait declares on it: the trait's dictionary has the
// associated type's, as it has a supertrait's.
trait Labeled {
    type Label: Display;
    fn label(&self) -> Self::Label;
}

impl Labeled for Count {
    type Label = u32;
    fn label(&self) -> u32 {
        self.n
    }
}

impl Labeled for Words {
    type Label = String;
    fn label(&self) -> String {
        self.list.join("+")
    }
}

fn tag<L: Labeled>(l: &L) -> String {
    format!("<{}>", l.label())
}

fn all<I: Iterator>(i: I) -> Vec<I::Item> {
    i.collect()
}

fn main() {
    println!("{:?}", drain(&mut Count { n: 3 }));
    println!("{:?}", drain(&mut Words { list: vec!["a".to_string(), "b".to_string()] }));
    println!("{} {}", total(&mut Count { n: 4 }), shown(&mut Words { list: vec!["x".to_string()] }));
    println!("{} {}", tag(&Count { n: 7 }), tag(&Words { list: vec!["p".to_string(), "q".to_string()] }));
    println!("{:?}", all(vec![1u32, 2].into_iter().map(|n| n * 10)));
    let mut source: Box<dyn Source<Item = u32>> = Box::new(Count { n: 2 });
    println!("{:?} {:?}", source.next_item(), source.next_item());
}
