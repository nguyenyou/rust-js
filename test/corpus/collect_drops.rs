// `collect()` of a chain that owns its items (ADR 0098): `map` takes each,
// `filter` and `skip_while` drop what they discard as they discard it, and
// what's collected is the new `Vec`'s.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    let v = vec![Noisy("a"), Noisy("bb"), Noisy("c"), Noisy("dd")];
    let long: Vec<Noisy> = v.into_iter().filter(|n| n.0.len() > 1).collect();
    println!("kept {}", long.len());

    let w = [Noisy("x"), Noisy("y"), Noisy("z")];
    let rest: Vec<Noisy> = w.into_iter().skip_while(|n| n.0 != "y").collect();
    println!("rest {}", rest.len());

    let mut fragments = vec![Noisy("f1"), Noisy("f2")];
    let none: Vec<Noisy> = std::mem::replace(&mut fragments, vec![]).into_iter().skip_while(|_| true).collect();
    println!("none {} {}", none.len(), fragments.len());

    let names: Vec<&str> = vec![Noisy("m1"), Noisy("m2")].into_iter().map(|n| n.0).collect();
    println!("{:?}", names);

    let checked: Vec<Noisy> = vec![Noisy("p1"), Noisy("p2"), Noisy("p3")]
        .into_iter()
        .filter(|n| n.0 != "p2")
        .map(|n| {
            println!("map {}", n.0);
            n
        })
        .collect();
    println!("checked {}", checked.len());
    println!("end");
}
