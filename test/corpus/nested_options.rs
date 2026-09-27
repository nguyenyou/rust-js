//@ ignore-rust-js: nested options, `Option<Option<T>>` (ROADMAP M7.2)
// `Option`s inside `Option`s, `Vec`s and structs keep `None` and
// `Some(None)` apart.
#[derive(Debug, Clone, PartialEq, Default)]
struct Config {
    name: Option<String>,
    limit: Option<Option<u32>>,
    tags: Vec<Option<String>>,
}

fn lookup(v: &[Option<i32>], i: usize) -> Option<Option<i32>> {
    v.get(i).copied()
}

fn main() {
    let items = vec![Some(1), None, Some(3)];
    for i in 0..4 {
        println!("{i}: {:?}", lookup(&items, i));
    }
    let flat: Vec<i32> = items.iter().flatten().copied().collect();
    println!("{flat:?} {:?}", items.iter().map(|x| x.map(|n| n * 2)).collect::<Vec<_>>());

    let mut config = Config::default();
    println!("{config:?}");
    config.limit = Some(None);
    config.tags = vec![None, Some("a".to_string())];
    println!("{config:?} {}", config == Config::default());
    config.limit = Some(Some(7));
    if let Some(Some(n)) = config.limit {
        println!("limit {n}");
    }
    let nested: Option<Option<Option<bool>>> = Some(Some(None));
    println!("{nested:?} {:?} {:?}", nested.flatten(), nested.flatten().flatten());
    let unset: Option<Option<u32>> = None;
    println!("{}", match unset {
        None => "unset",
        Some(None) => "no limit",
        Some(Some(_)) => "limit",
    });
}
