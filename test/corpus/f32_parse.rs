// `s.parse::<f32>()` reads the digits to their nearest `f32`, as Rust's
// does (ADR 0122). Through an `f64` that's right but where the digits, a
// hair either side of a tie between two `f32`s, are read as the tie itself:
// those are decided by the digits, compared with the tie exactly.
fn show(s: &str) {
    match s.parse::<f32>() {
        Ok(x) => println!("{:?} -> {} {:?}", s, x, x),
        Err(e) => println!("{:?} -> error: {}", s, e),
    }
}

fn main() {
    for s in ["0.1", "1.5", "-2.75", "3.4028235e38", "1e-45", "7e-46", "1e-50", "+12", ".5", "5.", "1E3"] {
        show(s);
    }
    // Past the largest `f32`, and its tie with infinity.
    for s in ["3.4028236e38", "1e39", "340282356779733661637539395458142568448", "340282356779733661637539395458142568447"] {
        show(s);
    }
    for s in ["inf", "-Infinity", "NaN", "", "abc", "1.2.3", "1e", "0x10", " 1"] {
        show(s);
    }
    // The tie between 1 and the next `f32`, 1 + 2^-24, and a hair either side.
    for s in [
        "1.000000059604644775390625",
        "1.000000059604644775390625000001",
        "1.000000059604644775390624999999",
        // The next tie up, whose even `f32` is the one above.
        "1.000000178813934326171875",
        "1.000000178813934326171874999999",
        "1.000000178813934326171875000001",
        // A tie among the subnormals, and with zero.
        "7.00649232162408535461864791644958065640130970938257885878534141944895541342930300743319094181060791015625e-46",
        "7.00649232162408535461864791644958065640130970938257885878534141944895541342930300743319094181060791015626e-46",
    ] {
        show(s);
    }
}
