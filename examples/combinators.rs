// `Option`'s and `Result`'s combinators, iterator adapters and consumers,
// and `Vec`'s methods (ADR 0062). A closure whose body is one value is
// written in place: `h.unwrap_or_else(|| 99)` is `h ?? 99`.

fn half(n: u32) -> Option<u32> {
    if n % 2 == 0 { Some(n / 2) } else { None }
}
fn parse(c: char) -> Result<u32, String> {
    if c == '1' {
        Ok(1)
    } else if c == '2' {
        Ok(2)
    } else {
        Err(format!("bad {c}"))
    }
}
pub fn options(n: u32) -> (u32, u32, u32, Option<u32>, Option<u32>, bool) {
    let h = half(n);
    (
        h.unwrap_or_else(|| 99),
        h.unwrap_or_default(),
        h.map_or(7, |x| x * 3),
        h.and_then(half),
        h.filter(|&x| x > 2),
        h.is_some_and(|x| x < 5),
    )
}
pub fn more_options(n: u32) -> (String, String, Option<u32>, bool, u32) {
    let h = half(n);
    let r = h.ok_or("odd".to_string());
    let s = h.ok_or_else(|| format!("odd {n}"));
    (
        format!("{r:?}"),
        format!("{s:?}"),
        h.or(Some(0)),
        h.is_none_or(|x| x > 1),
        h.map_or_else(|| 1000, |x| x + 1),
    )
}
pub fn results(c: char) -> (String, String, u32, u32, Option<String>, bool) {
    let r = parse(c);
    (
        format!("{:?}", r.clone().map(|x| x * 10)),
        format!("{:?}", r.clone().map_err(|e| e.chars().count())),
        r.clone()
            .and_then(|x| if x > 1 { Ok(x) } else { Err("small".to_string()) })
            .unwrap_or(0),
        r.clone().unwrap_or_else(|e| e.chars().count() as u32),
        r.clone().err(),
        r.is_ok_and(|x| x == 2),
    )
}
pub fn iters(n: u32) -> (Vec<u32>, Vec<u32>, Vec<(u32, char)>, Vec<u32>, Vec<u32>, Vec<u32>) {
    let v: Vec<u32> = (0..n).collect();
    (
        v.iter().filter_map(|&x| half(x)).collect(),
        v.iter().flat_map(|&x| vec![x, x]).collect(),
        v.iter().copied().zip(['a', 'b', 'c']).collect(),
        v.iter().copied().chain(vec![100, 200]).collect(),
        v.iter().copied().take_while(|&x| x < 3).collect(),
        v.iter().copied().skip_while(|&x| x < 3).step_by(2).collect(),
    )
}
pub fn consumers(
    n: u32,
) -> (
    Option<u32>,
    Option<u32>,
    u32,
    Option<u32>,
    Option<u32>,
    (Vec<u32>, Vec<u32>),
) {
    let v: Vec<u32> = (1..n + 1).collect();
    (
        v.iter().copied().max_by_key(|&x| (x % 3, x)),
        v.iter().copied().min_by(|a, b| b.cmp(a)),
        v.iter().product(),
        v.iter().copied().nth(2),
        v.iter().find_map(|&x| if x > 2 { Some(x * 100) } else { None }),
        v.iter().partition(|&&x| x % 2 == 0),
    )
}
pub fn vecs(n: u32) -> (bool, Vec<u32>, u32, Vec<Vec<u32>>, Vec<Vec<u32>>, Vec<u32>) {
    let mut v = vec![1u32, 1, 2, 3, 3, 3, 4];
    let has = v.contains(&n);
    v.dedup();
    v.insert(1, 50);
    let removed = v.remove(0);
    v.swap(0, 1);
    v.extend(vec![7, 8]);
    v.truncate(6);
    let w = v.windows(2).map(|w| w.to_vec()).collect();
    let c = v.chunks(4).map(|c| c.to_vec()).collect();
    (has, v, removed, w, c, vec![vec![1u32, 2], vec![3]].concat())
}
pub fn panics(i: usize) -> u32 {
    let mut v = vec![1u32, 2];
    v.remove(i)
}
pub fn report() -> String {
    let mut out = String::new();
    for n in [0, 4, 5, 8] {
        out.push_str(&format!("{:?} {:?}\n", options(n), more_options(n)));
    }
    for c in ['1', '2', 'x'] {
        out.push_str(&format!("{:?}\n", results(c)));
    }
    for n in [0, 5] {
        out.push_str(&format!("{:?} {:?} {:?}\n", iters(n), consumers(n), vecs(n)));
    }
    out
}
