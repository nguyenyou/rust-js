// An `f32`'s shortest digits, as Rust shows them with `{}` and `{:?}` (ADR
// 0122), over every power of two it has, where the gap below is half the gap
// above, subnormals too, and values of every size from a seeded generator.
fn main() {
    // 2^-149 to 2^127, each from the one before: doubling is exact.
    let mut power = 1.0f32;
    for _ in 0..149 {
        power *= 0.5;
    }
    for _ in 0..277 {
        println!("{} {:?}", power, power);
        power *= 2.0;
    }

    // 24 random bits, scaled by a random power of two: exact, but where
    // it falls below the normal range, where it's rounded.
    let mut seed: u32 = 20261001;
    let mut next = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        seed
    };
    for _ in 0..3000 {
        let significand = (next() >> 8) as f32;
        let mut x = significand;
        let scale = next() % 270;
        if scale < 150 {
            for _ in 0..scale + 23 {
                x *= 0.5;
            }
        } else {
            for _ in 0..scale - 150 {
                x *= 2.0;
            }
        }
        if next() % 2 == 0 {
            x = -x;
        }
        println!("{} {:?}", x, x);
    }
}
