// An `f32` is a JS number holding its value, each result rounded to it with
// `Math.fround` (ADR 0122): `+`, `-`, `*`, `/` and `sqrt` of the exact values
// rounded once are what Rust's are. Shown as Rust shows them, the shortest
// digits that are this `f32` and no other.
use std::ops::Add;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Reading {
    celsius: f32,
}

fn total<T: Add<Output = T> + Copy>(values: &[T], zero: T) -> T {
    let mut sum = zero;
    for &v in values {
        sum = sum + v;
    }
    sum
}

fn main() {
    // Arithmetic, each step rounded: ten tenths aren't one.
    let tenth = 0.1f32;
    let mut sum = 0.0f32;
    for _ in 0..10 {
        sum += tenth;
    }
    println!("{} {} {} {}", sum, sum == 1.0, 0.1f32 + 0.2f32, 1.0f32 / 3.0);
    println!("{} {} {}", 16_777_216f32 + 1.0, 3.0f32 * 1.1, (2.0f32).sqrt());
    println!("{:?} {:?} {:?}", 1.0f32, 0.1f32, -0.0f32);

    // As Rust shows them: `{}` never with an exponent, `{:?}` with one far
    // from 1, and the shortest digits of this f32.
    println!("{} {:?}", f32::MAX, f32::MAX);
    println!("{} {:?}", f32::MIN_POSITIVE, f32::MIN_POSITIVE);
    println!("{} {:?} {:?} {:?}", f32::EPSILON, 1e-5f32, 3e16f32, 123456.7f32);
    println!("{} {} {}", f32::NAN, f32::INFINITY, -f32::INFINITY);
    println!("{:.3} {:.0} {:.2}", 0.1f32, 2.5f32, 1.005f32);

    // Casts: from f32 saturating, into f32 rounded once.
    println!("{} {} {} {}", 3.9f32 as u8, -1.5f32 as u8, 1e10f32 as i32, f32::NAN as i32);
    println!("{} {} {}", 16_777_217i32 as f32, u32::MAX as f32, 0.1f64 as f32);
    // Through an f64 this would round twice, to 2^60.
    let big: u64 = (1 << 60) + (1 << 36) + 1;
    println!("{} {}", big as f32, (big as f32) as f64);
    println!("{} {}", 0.1f32 as f64, f32::MAX as f64);

    // Methods: exact ones as they are, the others rounded.
    let x = -2.75f32;
    println!("{} {} {} {} {}", x.abs(), x.floor(), x.ceil(), x.round(), x.trunc());
    println!("{} {} {}", 1.1f32.powi(9), 2.0f32.powi(-3), 2.0f32.powf(10.0));
    println!("{} {} {}", 1.5f32.max(f32::NAN), 1.5f32.min(-3.0), 7.0f32.sqrt() * 7.0f32.sqrt());
    println!("{} {}", f32::NAN.is_nan(), (1.0f32 / 0.0).is_infinite());

    // In a struct, an iterator, a sort, and generic code.
    let readings = vec![Reading { celsius: 21.7 }, Reading { celsius: -3.2 }, Reading { celsius: 0.1 }];
    println!("{:?}", readings[0]);
    let temps: Vec<f32> = readings.iter().map(|r| r.celsius).collect();
    println!("{} {}", temps.iter().sum::<f32>(), temps.iter().product::<f32>());
    let mut sorted = temps.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("{:?}", sorted);
    println!("{} {}", total(&temps, 0.0), total(&[0.1f64, 0.2], 0.0));
}
