//! Runs examples/fib.rs natively and prints every result as JSON lines.
//! The JS test runs the same calls on the generated fib.js and compares.

#[path = "../examples/fib.rs"]
#[allow(dead_code)]
mod fib;

// `modules`: examples/modules/lib.rs, a crate split across files, linked
// with `--extern`. (It can't be pulled in with `#[path]` like fib.rs: its
// `crate::` paths must mean its own root.)

use std::panic::{self, UnwindSafe};

use fib::*;

fn main() {
    panic::set_hook(Box::new(|_| {}));

    for n in 0..=25 {
        case("fib", &[n], || fib(n as u32) as i64);
        case("fib_match", &[n], || fib_match(n as u32) as i64);
    }
    // Past n = 47 the result no longer fits in u32 and wraps.
    for n in 0..=60 {
        case("fib_iter", &[n], || fib_iter(n as u32) as i64);
        case("fib_loop", &[n], || fib_loop(n as u32) as i64);
    }
    for n in 0..=20 {
        case("nth_asc", &[n], || nth(Order::Ascending, n as u32) as i64);
        case("nth_desc", &[n], || nth(Order::Descending, n as u32) as i64);
    }
    for x in [0, 1, -1, 715_827_882, 715_827_883, i32::MAX, i32::MIN, 1_000_000_000] {
        case("wrap_demo", &[x as i64], || wrap_demo(x) as i64);
    }
    for (a, b) in [(7, 2), (-7, 2), (7, -2), (i32::MIN, 1), (i32::MAX, -1), (5, 0), (i32::MIN, -1)] {
        case("ratio", &[a as i64, b as i64], || ratio(a, b) as i64);
    }

    for (a, b) in [(0, 0), (3, 5), (10, 20), (999, 1001), (2000, 3000), (65_535, 7)] {
        case("modules.summary", &[a as i64, b as i64], || modules::summary(a, b) as i64);
        case("modules.doubled_mean", &[a as i64, b as i64], || modules::doubled_mean(a, b) as i64);
        case("modules.stats.mean", &[a as i64, b as i64], || modules::stats::mean(a, b) as i64);
    }
    for x in [0, 1, 7, 999, 1000, 5000, u32::MAX] {
        case("modules.mixed", &[x as i64], || modules::mixed(x) as i64);
        case("modules.shadowed", &[x as i64], || modules::shadowed(x) as i64);
        case("modules.util.double", &[x as i64], || modules::util::double(x) as i64);
    }
}

fn case(name: &str, args: &[i64], f: impl FnOnce() -> i64 + UnwindSafe) {
    let outcome = match panic::catch_unwind(f) {
        Ok(v) => format!("\"value\":{v}"),
        Err(e) => {
            let msg = e.downcast_ref::<&str>().map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            format!("\"panic\":{msg:?}")
        }
    };
    println!("{{\"fn\":{name:?},\"args\":{args:?},{outcome}}}");
}
