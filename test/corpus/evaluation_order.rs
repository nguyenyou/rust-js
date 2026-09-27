//@ run-fail: attempt to divide by zero
// Operands, arguments and fields are evaluated left to right, and what ran
// before a panic has happened: its output is compared too.
fn note(log: &mut Vec<String>, name: &str, value: i32) -> i32 {
    log.push(name.to_string());
    println!("evaluated {name}");
    value
}

fn three(a: i32, b: i32, c: i32) -> i32 {
    a * 100 + b * 10 + c
}

struct Pair {
    left: i32,
    right: i32,
}

fn main() {
    let mut log = Vec::new();
    let sum = note(&mut log, "a", 1) + note(&mut log, "b", 2) * note(&mut log, "c", 3);
    println!("sum {sum}");
    let args = three(note(&mut log, "x", 1), note(&mut log, "y", 2), note(&mut log, "z", 3));
    println!("args {args}");
    let pair = Pair { right: note(&mut log, "right", 2), left: note(&mut log, "left", 1) };
    println!("pair {} {}", pair.left, pair.right);
    println!("{log:?}");
    let zero = log.len() as i32 - 8;
    let _ = note(&mut log, "before", 1) / zero + note(&mut log, "never", 2);
    println!("unreachable");
}
