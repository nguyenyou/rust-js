// 64-bit integers (ADR 0086): IDs, hashes, timestamps and money, which most
// APIs keep in an `i64` or a `u64`. Each is a JS BigInt, so it holds every
// value Rust's does, past the 2^53 a JS number is exact to, and `+`, `*` and
// `<<` wrap as release Rust's do. JSON reads and writes them to the digit.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An ID of a millisecond, a worker and a sequence, packed in 64 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Id(pub u64);

impl Id {
    pub const EPOCH: u64 = 1_288_834_974_657;

    pub fn new(millis: u64, worker: u16, sequence: u16) -> Id {
        let worker = u64::from(worker & 0x3ff);
        let sequence = u64::from(sequence & 0xfff);
        Id(((millis - Self::EPOCH) << 22) | (worker << 12) | sequence)
    }
    pub fn millis(self) -> u64 {
        (self.0 >> 22) + Self::EPOCH
    }
    pub fn worker(self) -> u16 {
        ((self.0 >> 12) & 0x3ff) as u16
    }
    pub fn sequence(self) -> u16 {
        (self.0 & 0xfff) as u16
    }
}

/// FNV-1a: a multiply that wraps at every character.
pub fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for c in text.chars() {
        hash ^= c as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Payment {
    pub id: Id,
    pub cents: i64,
    pub at: i64,
}

/// Each account's total, in cents.
pub fn balances(entries: &[(&str, i64)]) -> BTreeMap<String, i64> {
    let mut totals = BTreeMap::new();
    for &(account, cents) in entries {
        *totals.entry(account.to_string()).or_insert(0) += cents;
    }
    totals
}

pub fn dollars(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let cents = cents.unsigned_abs();
    format!("{sign}${}.{:02}", cents / 100, cents % 100)
}

pub fn report() -> String {
    let mut out = String::new();

    let id = Id::new(1_700_000_000_123, 5, 42);
    out.push_str(&format!("{:?} {} {} {}\n", id, id.millis(), id.worker(), id.sequence()));
    let later = Id::new(1_700_000_000_124, 1, 0);
    out.push_str(&format!("{} {:?}\n", id < later, id.0.max(later.0) == later.0));

    for text in ["", "a", "rust-js"] {
        out.push_str(&format!("{:016x} ", fnv1a(text)));
    }
    out.push('\n');

    let entries = [("rent", -250_000_00i64), ("pay", 1_000_000_00), ("rent", -250_000_00), ("big", i64::MAX / 2)];
    for (account, cents) in balances(&entries) {
        out.push_str(&format!("{account} {} ", dollars(cents)));
    }
    out.push('\n');

    // The edges, where a JS number would round.
    let max = u64::MAX;
    let min = i64::MIN;
    out.push_str(&format!(
        "{} {} {} {} {}\n",
        max,
        max.wrapping_add(1),
        min.wrapping_sub(1),
        (1u64 << 53) + 1,
        9_007_199_254_740_993i64 * 3
    ));
    out.push_str(&format!(
        "{} {} {} {} {} {}\n",
        -1i64 as u64,
        max as i64,
        max as u32,
        min as i32,
        max as f64,
        (1i64 << 60) as f64
    ));
    out.push_str(&format!(
        "{} {} {} {}\n",
        f64::MAX as i64,
        -1.5f64 as u64,
        f64::NAN as i64,
        1e19 as u64
    ));
    out.push_str(&format!(
        "{:?} {:?} {:?} {:?}\n",
        max.checked_add(1),
        0i64.checked_sub(min),
        5i64.checked_div(0),
        10u64.checked_pow(19)
    ));
    out.push_str(&format!(
        "{} {} {} {} {}\n",
        max.saturating_add(9),
        min.saturating_sub(1),
        max.leading_zeros(),
        (1u64 << 40).trailing_zeros(),
        max.count_ones()
    ));
    out.push_str(&format!(
        "{} {} {:?} {:?}\n",
        (-7i64).rem_euclid(3),
        (-7i64).div_euclid(3),
        "18446744073709551615".parse::<u64>(),
        "18446744073709551616".parse::<u64>().is_err()
    ));
    out.push_str(&format!(
        "{:?} {:?} {:?}\n",
        u32::try_from(max),
        i64::try_from(1u64 << 63).is_err(),
        u16::try_from(min.wrapping_add(min)).is_ok()
    ));

    let mut stamps: Vec<i64> = vec![1_700_000_000_000, -5, 0, i64::MAX, i64::MIN];
    stamps.sort();
    let total: i64 = stamps.iter().skip(1).take(3).sum();
    out.push_str(&format!("{stamps:?} {total} {:?}\n", stamps.iter().max()));

    let payment = Payment { id, cents: -1_999, at: 1_700_000_000_123 };
    let text = serde_json::to_string(&payment).unwrap();
    out.push_str(&format!("{text}\n"));
    let back: Payment = serde_json::from_str(&text).unwrap();
    out.push_str(&format!("{:?}\n", back == payment));
    for text in [
        r#"{"id":18446744073709551615,"cents":-9223372036854775808,"at":0}"#,
        r#"{"id":-1,"cents":0,"at":0}"#,
        r#"{"id":1,"cents":9223372036854775808,"at":0}"#,
        r#"{"id":1.5,"cents":0,"at":0}"#,
    ] {
        match serde_json::from_str::<Payment>(text) {
            Ok(p) => out.push_str(&format!("{p:?}\n")),
            Err(e) => out.push_str(&format!("{e}\n")),
        }
    }
    out
}

/// What panics: dividing by zero, and `i64::MIN / -1`, whose answer an
/// `i64` can't hold, even in release.
pub fn panics(i: u32) -> i64 {
    let zero = i64::from(i) - i64::from(i);
    match i {
        0 => 7 / zero,
        1 => i64::MIN / (zero - 1),
        2 => 7 % zero,
        _ => "x".parse::<i64>().unwrap(),
    }
}
