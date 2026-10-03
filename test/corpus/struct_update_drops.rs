// A struct update from a value with a destructor (ADR 0098): `..base`
// moves the fields it doesn't name out of `base`, a part move, once every
// field's value is made. What `base` keeps is dropped where it ends: a
// variable's with the variable, a temporary's with its scope.
struct Noisy(u8);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

struct Partial {
    x: Noisy,
    y: Noisy,
    n: u8,
}

impl Default for Partial {
    fn default() -> Self {
        println!("default");
        Partial { x: Noisy(20), y: Noisy(21), n: 0 }
    }
}

fn swap_y(p: Partial) -> Partial {
    Partial { y: keep(p.y), ..p }
}

fn keep(n: Noisy) -> Noisy {
    println!("keep {}", n.0);
    Noisy(n.0 + 10)
}

fn rebuild(p: Partial, stop: bool) -> Option<Partial> {
    Some(Partial {
        y: if stop {
            return None;
        } else {
            Noisy(9)
        },
        ..p
    })
}

fn main() {
    {
        let p = Partial { x: Noisy(1), y: Noisy(2), n: 1 };
        let q = Partial { y: Noisy(3), ..p };
        println!("q {} {} {}", q.x.0, q.y.0, q.n);
    }
    println!("after p");

    let swapped = swap_y(Partial { x: Noisy(4), y: Noisy(5), n: 2 });
    println!("swapped {} {}", swapped.x.0, swapped.y.0);

    let from_temp = Partial { y: Noisy(6), ..Partial { x: Noisy(7), y: Noisy(8), n: 3 } };
    println!("from temp {} {} {}", from_temp.x.0, from_temp.y.0, from_temp.n);

    let defaults = Partial { x: Noisy(22), ..Default::default() };
    println!("defaults {} {}", defaults.x.0, defaults.y.0);

    println!("stopped {}", rebuild(Partial { x: Noisy(10), y: Noisy(11), n: 4 }, true).is_none());
    match rebuild(Partial { x: Noisy(12), y: Noisy(13), n: 5 }, false) {
        Some(built) => println!("built {} {}", built.x.0, built.y.0),
        None => println!("none"),
    }
    println!("end");
}
