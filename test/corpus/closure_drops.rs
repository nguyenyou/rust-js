// A closure that holds a value with a destructor (ADR 0098): one that
// consumes what it holds drops what it didn't move as its call ends, and
// any closure dropped uncalled drops what it holds then.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn call_once<F: FnOnce() -> usize>(f: F) -> usize {
    println!("calling");
    f()
}

fn keep<F: FnOnce() -> usize>(_f: F) {
    println!("not called");
}

fn run<F: FnOnce()>(f: F) {
    f()
}

fn main() {
    let a = Noisy("a");
    let tick = move || {
        println!("tick {}", a.0);
        drop(a);
        1
    };
    println!("before");
    println!("{}", tick());
    println!("after");

    let b = Noisy("b");
    let c = Noisy("c");
    let once = move || {
        let kept = b;
        let held = &c;
        println!("once {} {}", kept.0, held.0);
        2
    };
    println!("{}", once());

    {
        let d = Noisy("d");
        let d2 = Noisy("d2");
        let _unused = move || {
            drop(d);
            let _kept = &d2;
            3
        };
        println!("made");
    }

    let p = Noisy("p");
    let q = Noisy("q");
    run(move || {
        let (x, y) = (&p, &q);
        println!("both {} {}", x.0, y.0);
    });

    let mut r = Noisy("r1");
    let take = move || drop(r);
    r = Noisy("r2");
    take();
    println!("{}", r.0);

    let e = Noisy("e");
    println!("{}", call_once(move || {
        let x = e;
        x.0.len()
    }));
    let f = Noisy("f");
    keep(move || {
        drop(f);
        4
    });

    {
        let g = Noisy("g");
        let read = move || {
            let held = &g;
            held.0.len()
        };
        println!("{} {}", read(), read());
    }

    for i in 0..2 {
        let h = Noisy(if i == 0 { "h0" } else { "h1" });
        let maybe = move || drop(h);
        if i == 1 {
            maybe();
        }
        println!("loop {}", i);
    }
    println!("end");
}
