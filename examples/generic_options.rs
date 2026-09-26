// `Option<T>` in generic code (ADR 0051). `Some(x)` is `x`, unless `x`
// could itself look like `None`: with `T = ()` or `T = Option<i32>`, a
// `Some(x)` is a box, so it's still `Some`.

/// `Some` of each element, counted: with `T = ()`, `Some(())` must be `Some`.
pub fn count_some<T>(xs: Vec<T>) -> u32 {
    let mut n = 0;
    for x in xs {
        let o = Some(x);
        if o.is_some() {
            n += 1;
        }
    }
    n
}

pub fn pick<T>(x: T, keep: bool) -> Option<T> {
    if keep { Some(x) } else { None }
}

/// Matching what `pick` made: `Some(None)` is `Some`.
pub fn kept<T>(xs: Vec<T>) -> u32 {
    let mut n = 0;
    for x in xs {
        match pick(x, true) {
            Some(_) => n += 1,
            None => {}
        }
    }
    n
}

/// The value inside, taken out again: `unwrap_or`, `unwrap` and `map`.
pub fn inner_or<T>(x: T, keep: bool, fallback: T) -> T {
    pick(x, keep).unwrap_or(fallback)
}

pub fn unwrapped<T>(x: T) -> T {
    pick(x, true).unwrap()
}

pub fn mapped<T, U>(x: T, f: impl Fn(T) -> U) -> Option<U> {
    pick(x, true).map(f)
}

/// `?` on an `Option<T>`.
pub fn tried<T>(x: T, keep: bool) -> Option<T> {
    let v = pick(x, keep)?;
    Some(v)
}

/// What std makes an `Option` of: the last of a `Vec`, the first of a slice.
pub fn popped<T>(mut xs: Vec<T>) -> Option<T> {
    xs.pop()
}

pub fn first_of<T>(xs: &[T]) -> Option<&T> {
    xs.first()
}

// A concrete `Option<Option<i32>>` is still an error (ADR 0030), so these
// ask about the generic ones without making one.
pub fn tried_some<T>(x: T, keep: bool) -> bool {
    tried(x, keep).is_some()
}

pub fn popped_some<T>(xs: Vec<T>) -> bool {
    popped(xs).is_some()
}

pub fn first_some<T>(xs: &[T]) -> bool {
    first_of(xs).is_some()
}

// With the types a JS value of can't tell from `None`. Each result is
// concrete, so native Rust and the JS can be compared.

pub fn units() -> u32 {
    count_some(vec![(), (), ()]) + kept(vec![(), ()])
}

pub fn nones() -> u32 {
    count_some::<Option<i32>>(vec![None, Some(1)]) + kept::<Option<i32>>(vec![None, None, Some(2)])
}

pub fn inner_values() -> (Option<i32>, Option<i32>, Option<i32>, i32) {
    let a: Option<i32> = inner_or(None, true, Some(7));
    let b: Option<i32> = inner_or(Some(3), false, Some(7));
    let c: Option<i32> = unwrapped(None);
    (a, b, c, inner_or(5, true, 9))
}

pub fn mapped_values() -> (bool, bool, i32) {
    let none_inside = mapped(None::<i32>, |o| o.is_none()).unwrap();
    let doubled = mapped(4, |n: i32| n * 2).unwrap();
    (none_inside, tried_some(None::<i32>, true), doubled)
}

pub fn std_values() -> (bool, bool, bool, i32) {
    let popped_none = popped_some(vec![Some(1), None]);
    let empty = popped_some::<Option<i32>>(Vec::new());
    let first_none = first_some(&[None, Some(1)]);
    (popped_none, empty, first_none, popped(vec![1, 2, 3]).unwrap())
}
