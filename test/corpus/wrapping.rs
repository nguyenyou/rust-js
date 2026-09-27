// Integers wrap at their width, as release Rust's do, at each edge.
fn main() {
    let (a, b, c, d) = (u8::MAX, i8::MIN, u16::MAX, i16::MAX);
    println!("{} {} {} {}", a.wrapping_add(1), b.wrapping_sub(1), c.wrapping_mul(2), d.wrapping_add(1));
    let (e, f) = (u32::MAX, i32::MIN);
    println!("{} {} {}", e.wrapping_add(2), f.wrapping_sub(1), e.wrapping_mul(e));
    let (g, h) = (u64::MAX, i64::MIN);
    println!("{} {} {}", g.wrapping_add(1), h.wrapping_sub(1), g.wrapping_mul(3));
    println!("{} {} {}", 200u8 as i8, -1i32 as u32, 300i32 as u8);
    println!("{} {} {}", 1u32 << 31, 1i32 << 31, (-8i32) >> 1);
    println!("{:?} {:?} {:?}", 250u8.checked_add(10), 5u32.checked_sub(6), i32::MAX.checked_mul(2));
    println!("{} {} {}", 250u8.saturating_add(10), 0u32.saturating_sub(1), i32::MIN.saturating_sub(1));
    shifts(1, 9, 3);
}

// A shift's amount may be any integer type, and is masked to the width.
fn shifts(by: i64, far: u64, small: u8) {
    println!("{} {} {} {}", 22i32 >> by, 1u8 << far, 5u64 << small, (-64i64) >> far);
}
