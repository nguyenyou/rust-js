// A field's type can be a trait's associated type, `K::Value`: where the
// struct is used, it's the type that stands for, `Option<u32>`, as rustc's
// own types of its places are.
trait Key {
    type Value;
}

impl Key for i32 {
    type Value = Option<u32>;
}

impl Key for &'static str {
    type Value = Vec<String>;
}

#[derive(Debug, Clone, PartialEq)]
struct Node<K: Key> {
    key: K,
    value: K::Value,
}

enum Slot<K: Key> {
    Empty,
    Full(K::Value),
}

fn get<K: Key<Value = Option<V>>, V: Clone>(node: &Node<K>) -> Option<V> {
    node.value.clone()
}

fn fill(slot: Slot<i32>) -> u32 {
    match slot {
        Slot::Full(Some(n)) => n,
        Slot::Full(None) => 1,
        Slot::Empty => 0,
    }
}

fn main() {
    let a: Node<i32> = Node { key: 1, value: Some(22) };
    let b = Node { key: "k", value: vec!["x".to_string()] };
    println!("{:?} {:?} {:?}", get(&a), a, b);
    let c = a.clone();
    println!("{} {}", c == a, c.value.map_or(0, |n| n + 1));
    println!("{} {} {}", fill(Slot::Full(Some(5))), fill(Slot::Full(None)), fill(Slot::Empty));
}
