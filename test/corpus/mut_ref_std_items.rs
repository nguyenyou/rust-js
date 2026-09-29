// A std call's `&mut` to a number or a `String`, `get_mut`'s say, is the
// item itself, not a cell (ADR 0099): matched, its binding reads the item.
// A crate's own cells given through a std call stay cells. Found by rustc's
// `nll/process_or_insert_default.rs`, whose binding was read as a cell.
use std::collections::HashMap;

fn process(x: &str) -> usize {
    x.chars().count()
}

fn process_or_insert_default(map: &mut HashMap<usize, String>, key: usize) -> usize {
    match map.get_mut(&key) {
        Some(value) => process(value),
        None => {
            map.insert(key, "".to_string());
            0
        }
    }
}

fn main() {
    let mut map = HashMap::new();
    map.insert(22, format!("Hello, world"));
    println!("{} {}", process_or_insert_default(&mut map, 22), process_or_insert_default(&mut map, 66));
    println!("{:?}", map.get(&66));

    let mut m: HashMap<u32, i32> = HashMap::new();
    m.insert(1, 5);
    match m.get_mut(&1) {
        Some(v) => println!("{}", *v + 1),
        None => {}
    }
    if let Some(v) = m.get_mut(&1) {
        *v += 1;
    }
    println!("{:?}", m.get(&1));

    let (mut a, mut b) = (1, 2);
    let mut refs = vec![&mut a, &mut b];
    if let Some(r) = refs.pop() {
        *r += 5;
    }
    for r in refs {
        *r += 10;
    }
    let mut c = 3;
    let o = Some(&mut c);
    let r = o.unwrap();
    *r += 1;
    println!("{a} {b} {c}");
}
