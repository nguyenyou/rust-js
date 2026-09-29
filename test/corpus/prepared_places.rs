use std::collections::HashMap;

fn key(log: &mut Vec<i32>) -> i32 {
    log.push(1);
    0
}

fn value(log: &mut Vec<i32>) -> i32 {
    log.push(2);
    3
}

fn slots(take: bool) {
    let mut map = HashMap::new();
    map.insert(0, 1);
    if let Some(n) = map.get_mut(&0) {
        // Statement-valued assignment must still write the map entry back.
        *n = if take {
            let next = *n + 3;
            next
        } else {
            let next = *n + 5;
            next
        };
        *n += 2;
        let rhs = 3;
        *n += &rhs;
    }
    println!("{}", map[&0]);
}

fn main() {
    slots(false);
    slots(true);
    let mut map = HashMap::new();
    let mut log = Vec::new();
    // Primitive compound assignment takes the value first.
    *map.entry(key(&mut log)).or_insert(10) += value(&mut log);
    println!("{:?} {}", log, map[&0]);
    log.clear();
    // An overloaded operator is a call: receiver, then argument.
    *map.entry(key(&mut log)).or_insert(10) += &value(&mut log);
    println!("{:?} {}", log, map[&0]);
}
