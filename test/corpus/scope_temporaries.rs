// A temporary with a destructor ends where rustc's scope tree ends it
// (ADR 0098): an `if` or `while` condition's after the condition, a
// block's tail's after the tail, before the block's own variables, and
// `assert_eq!`'s, whose `match` is its block's tail. A function's tail,
// or a closure's, is one: an operand there that a `return` leaves behind
// is dropped as it leaves.
#[derive(PartialEq, Debug)]
struct Noisy(u8);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

impl Noisy {
    fn big(&self) -> bool {
        println!("big {}", self.0);
        self.0 > 2
    }
}

fn make(n: u8) -> Noisy {
    println!("make {n}");
    Noisy(n)
}

fn tail(n: u8) -> bool {
    let _kept = Noisy(100 + n);
    make(n).big()
}

struct Pair {
    a: Noisy,
    b: u8,
}

fn early(stop: bool) -> Option<Pair> {
    Some(Pair {
        a: make(9),
        b: if stop {
            return None;
        } else {
            1
        },
    })
}

fn kept_early(stop: bool) -> Option<Pair> {
    let _kept = Noisy(30);
    Some(Pair {
        a: make(31),
        b: if stop {
            return None;
        } else {
            3
        },
    })
}

fn main() {
    if make(1).big() {
        println!("then");
    } else {
        println!("else");
    }

    let mut n = 2;
    while make(n).big() == false {
        println!("round {n}");
        n += 1;
    }

    let found = {
        let _inner = Noisy(50);
        make(5).big()
    };
    println!("found {found}");

    println!("tail {}", tail(6));

    assert_eq!(make(7), Noisy(7));
    assert!(make(8).big());
    println!("early {}", early(true).is_none());
    match early(false) {
        Some(pair) => println!("late {}", pair.b),
        None => println!("late none"),
    }

    let check = |n: u8| make(n).big();
    println!("check {}", check(10));
    let pair = |stop: bool| -> Option<Pair> {
        Some(Pair {
            a: make(11),
            b: if stop {
                return None;
            } else {
                2
            },
        })
    };
    println!("pair {}", pair(true).is_none());
    println!("kept {}", kept_early(true).is_none());
    println!("end");
}
