// A field of what a call's `&mut` points to is written where it is: the
// reference is the object (ADR 0025), whether std's method returned it or a
// user's `DerefMut` did. Rust runs the right side first, then the call.
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Item {
    name: String,
    count: u32,
}

fn pick<'a>(items: &'a mut Vec<Item>, log: &mut Vec<&'static str>) -> &'a mut Item {
    log.push("pick");
    items.iter_mut().last().unwrap()
}

fn value(log: &mut Vec<&'static str>) -> u32 {
    log.push("value");
    5
}

fn main() {
    let mut items = vec![Item { name: "a".into(), count: 1 }, Item { name: "b".into(), count: 2 }];
    items.iter_mut().last().unwrap().count = 9;
    items.iter_mut().nth(1).unwrap().count += 1;
    let mut log = Vec::new();
    pick(&mut items, &mut log).count = value(&mut log);
    println!("{:?} {:?}", items, log);

    let mut stock: HashMap<&str, Item> = HashMap::new();
    stock.insert("pen", Item { name: "pen".into(), count: 3 });
    stock.get_mut("pen").unwrap().count *= 4;
    println!("{}", stock["pen"].count);
}
