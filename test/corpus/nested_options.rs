// An `Option` of what can look like `None`, an `Option<Option<i32>>` or an
// `Option<()>`, boxes a `Some` that would (ADR 0051): `Some(None)` is
// `{ $someNone: 0 }`, and every other value is itself.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Unit;

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    name: String,
    note: Option<Option<String>>,
}

// Constants rustc computes are boxed the same way.
const NOTHING_FOUND: Option<Option<i32>> = Some(None);
const DEEP: Option<Option<Option<u8>>> = Some(Some(None));
const FOUND_UNIT: Option<&Unit> = Some(&Unit);

fn lookup(table: &[Option<i32>], i: usize) -> Option<Option<i32>> {
    table.get(i).copied()
}

fn depth(o: Option<Option<Option<u8>>>) -> u8 {
    match o {
        None => 0,
        Some(None) => 1,
        Some(Some(None)) => 2,
        Some(Some(Some(n))) => 3 + n,
    }
}

fn main() {
    let table = [Some(1), None];
    for i in 0..3 {
        println!("{:?}", lookup(&table, i));
    }
    let found: Option<()> = if table.len() > 1 { Some(()) } else { None };
    println!("{:?} {}", found, found.is_some());
    println!("{} {} {} {}", depth(None), depth(Some(None)), depth(Some(Some(None))), depth(Some(Some(Some(4)))));
    let units = vec![Some(Unit), None];
    let firsts: Vec<Option<&Option<Unit>>> = vec![units.first(), units.get(1), units.get(5)];
    println!("{:?}", firsts);
    let doubled = lookup(&table, 0).map(|inner| inner.map(|n| n * 2));
    println!("{:?} {:?} {:?}", doubled, lookup(&table, 1).unwrap_or(Some(9)), lookup(&table, 1) == Some(None));
    if let Some(Some(n)) = lookup(&table, 0) {
        println!("got {n}");
    }
    let e = Entry { name: "a".to_string(), note: Some(None) };
    let mut f = e.clone();
    f.note = Some(Some("n".to_string()));
    println!("{:?} {:?} {}", e, f, e == f);
    println!("{:?} {:?} {:?}", (table.len() > 1).then(|| ()), Some(1).map(|_| ()), Some(&()));
    let mut grid: Option<Option<[i32; 2]>> = Some(Some([1, 2]));
    let kept = grid;
    if let Some(Some(cells)) = &mut grid {
        cells[0] = 9;
    }
    println!("{:?} {:?}", kept, grid);
    println!("{:?} {} {:?} {:?}", NOTHING_FOUND, depth(DEEP), FOUND_UNIT, NOTHING_FOUND == lookup(&table, 1));
    let mut stack = vec![Some(Unit), None];
    while let Some(top) = stack.pop() {
        println!("{:?}", top);
    }
}
