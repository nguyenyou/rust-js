// A format's arguments are taken in order, before any is shown. An
// argument the JS names first, as an `Option` shown with `{:?}` is, mustn't
// run before those ahead of it read what it changes.

struct Counter {
    n: u32,
}

impl Counter {
    fn bump(&mut self) -> Option<u32> {
        self.n += 1;
        Some(self.n)
    }

    fn get(&self) -> u32 {
        self.n
    }
}

fn main() {
    let mut v = vec![1, 2, 3];
    println!("{} {:?} {}", v.len(), v.pop(), v.len());
    let mut c = Counter { n: 0 };
    println!("{} {:?} {} {:?}", c.get(), c.bump(), c.get(), c.bump());
    let mut words = vec!["a".to_string()];
    let line = format!("{}|{:?}|{}", words.join(","), words.pop(), words.len());
    println!("{line}");
}
