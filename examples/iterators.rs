// Iterators and sorting (ADR 0036): an iterator is a JS array, its adapters
// are the array's methods, and `Ordering` is -1, 0 or 1, as a comparator
// returns it.

use std::cmp::Ordering;

pub fn squares(n: u32) -> Vec<u32> {
    (0..n).map(|i| i * i).collect()
}

pub fn evens(v: &[i32]) -> Vec<i32> {
    v.iter().filter(|&&x| x % 2 == 0).copied().collect()
}

pub fn stats(v: &[i32]) -> (u32, i32, bool, bool, Option<i32>, Option<usize>) {
    let positive = v.iter().filter(|x| **x > 0).count() as u32;
    let sum: i32 = v.iter().sum();
    let any_negative = v.iter().any(|&x| x < 0);
    let no_zero = v.iter().all(|&x| x != 0);
    let big = v.iter().copied().find(|&x| x > 10);
    let three = v.iter().position(|&x| x == 3);
    (positive, sum, any_negative, no_zero, big, three)
}

pub fn extremes(v: &[i32]) -> (Option<i32>, Option<i32>) {
    (v.iter().copied().max(), v.iter().copied().min())
}

pub fn indexed(words: &[&str]) -> Vec<String> {
    words.iter().enumerate().map(|(i, w)| format!("{i}:{w}")).collect()
}

pub fn middle(v: &[i32]) -> Vec<i32> {
    v.iter().skip(1).take(2).rev().copied().collect()
}

pub fn non_empty(words: &[&str]) -> u32 {
    words.iter().fold(0, |count, w| if w.is_empty() { count } else { count + 1 })
}

pub fn shouted(words: &[&str]) -> String {
    words.iter().map(|w| w.to_uppercase()).collect::<Vec<_>>().join("-")
}

pub fn backwards(s: &str) -> String {
    s.chars().rev().collect()
}

/// `collect` makes a new `Vec`: sorting it leaves the original as it was.
pub fn sorted_copy(v: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
    let mut copy: Vec<i32> = v.iter().copied().collect();
    copy.sort();
    (v, copy)
}

pub fn sorted(v: &[i32]) -> Vec<i32> {
    let mut w = v.to_vec();
    w.sort();
    w
}

pub fn descending(v: &[i32]) -> Vec<i32> {
    let mut w = v.to_vec();
    w.sort_by(|a, b| b.cmp(a));
    w
}

/// By the last digit; stable, so ties keep their order, as in Rust.
pub fn by_last_digit(v: &[i32]) -> Vec<i32> {
    let mut w = v.to_vec();
    w.sort_by_key(|x| x % 10);
    w
}

pub fn sorted_words(words: &[&str]) -> Vec<String> {
    let mut w: Vec<String> = words.iter().map(|s| s.to_string()).collect();
    w.sort();
    w.reverse();
    w
}

/// Two keys: `then_with`.
pub fn by_length_then_name(words: &[&str]) -> Vec<String> {
    let mut w: Vec<String> = words.iter().map(|s| s.to_string()).collect();
    w.sort_by(|a, b| a.is_empty().cmp(&b.is_empty()).then_with(|| a.cmp(b)));
    w
}

pub fn compare(a: i32, b: i32) -> i32 {
    match a.cmp(&b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

pub fn bigger(a: u32, b: u32) -> (u32, u32) {
    (a.max(b), a.min(b))
}
