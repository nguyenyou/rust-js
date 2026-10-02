// A loop that owns its items (ADR 0098): each is its pattern's for a time
// round, and those it hasn't reached when it leaves early are dropped, in
// order, as Rust drops what its iterator still holds.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

struct Countdown(u8);

impl Iterator for Countdown {
    type Item = Noisy;
    fn next(&mut self) -> Option<Noisy> {
        self.0 = self.0.checked_sub(1)?;
        Some(Noisy(["c0", "c1", "c2"][self.0 as usize]))
    }
}

fn first_long(items: Vec<Noisy>) -> Option<&'static str> {
    for item in items {
        if item.0.len() > 2 {
            return Some(item.0);
        }
        println!("short {}", item.0);
    }
    None
}

fn main() {
    let v = vec![Noisy("a"), Noisy("b"), Noisy("c"), Noisy("d")];
    for n in v {
        if n.0 == "b" {
            continue;
        }
        if n.0 == "c" {
            break;
        }
        println!("got {}", n.0);
    }
    println!("after vec");

    for n in [Noisy("x"), Noisy("y")] {
        println!("array {}", n.0);
    }
    for n in Some(Noisy("some")).into_iter() {
        println!("option {}", n.0);
    }
    let none: Option<Noisy> = None;
    for n in none {
        println!("never {}", n.0);
    }
    for _ in std::iter::once(Noisy("unnamed")) {
        println!("body before the drop");
    }
    for (i, n) in vec![(1, Noisy("t1")), (2, Noisy("t2"))] {
        println!("pair {} {}", i, n.0);
    }
    for mut n in vec![Noisy("m1"), Noisy("m2")].into_iter() {
        if n.0 == "m1" {
            n = Noisy("m1b");
        } else {
            drop(n);
            println!("moved");
            continue;
        }
        println!("kept {}", n.0);
    }

    'outer: for row in vec![Noisy("r1"), Noisy("r2")] {
        for cell in vec![Noisy("x1"), Noisy("x2")] {
            if cell.0 == "x2" && row.0 == "r1" {
                break 'outer;
            }
            println!("cell {} {}", row.0, cell.0);
        }
    }

    for n in Countdown(3) {
        if n.0 == "c1" {
            break;
        }
        println!("count {}", n.0);
    }
    println!("{:?}", first_long(vec![Noisy("s"), Noisy("long"), Noisy("rest")]));
    println!("end");
}
