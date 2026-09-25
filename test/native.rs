//! Runs the examples natively and prints every result as JSON lines. The
//! JS test runs the same calls on the generated JS and compares.
//!
//! Values are printed the way ADR 0020 says JS holds them: a struct as an
//! object, a tuple (or tuple struct) as an array.

#[path = "../examples/fib.rs"]
#[allow(dead_code)]
mod fib;

#[path = "../examples/closures.rs"]
#[allow(dead_code)]
mod closures;

#[path = "../examples/structs.rs"]
#[allow(dead_code)]
mod structs;

// `modules`: examples/modules/lib.rs, a crate split across files, linked
// with `--extern`. (It can't be pulled in with `#[path]` like fib.rs: its
// `crate::` paths must mean its own root.)

use std::panic::{self, UnwindSafe};

use fib::*;
use structs::{Point, Rect, Size};

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

    let ints = [0, 1, -1, 7, -7, 100, i32::MAX, i32::MIN];
    for &x in &ints {
        for &y in &ints {
            case("structs.point", &[x as i64, y as i64], || structs::point(x, y));
            case("structs.moved", &[x as i64, y as i64, 3], || structs::moved(x, y, 3));
            case("structs.with_x", &[x as i64, y as i64], || structs::with_x(x, y));
            case("structs.written_order", &[x as i64, y as i64], || structs::written_order(x, y));
            case("structs.quadrant", &[x as i64, y as i64], || structs::quadrant(x, y) as i64);
            case_with("structs.classify", &[&(x, y)], || structs::classify((x, y)) as i64);
        }
        case("structs.copies_are_separate", &[x as i64], || structs::copies_are_separate(x));
        case("structs.caller_keeps_its_point", &[x as i64], || structs::caller_keeps_its_point(x));
        case("structs.moves_share_nothing", &[x as i64], || structs::moves_share_nothing(x));
        case("structs.bound_before_move", &[x as i64], || structs::bound_before_move(x));
    }
    for (w, h) in [(0, 0), (3, 4), (65_536, 65_536), (u32::MAX, 2)] {
        case("structs.rect", &[-1, 2, w as i64, h as i64], || structs::rect(-1, 2, w, h));
        case("structs.grow", &[w as i64, h as i64, 5], || structs::grow(w, h, 5));
        let r = structs::rect(0, 0, w, h);
        case_with("structs.area", &[&r], || structs::area(structs::rect(0, 0, w, h)) as i64);
    }
    for &a in &ints {
        case("closures.move_copies", &[a as i64], || closures::move_copies(a));
        case("closures.own_state", &[a as i64], || closures::own_state(a));
        case("closures.struct_copy", &[a as i64], || closures::struct_copy(a));
        for &b in &[0, 3, -5, i32::MAX] {
            case("closures.by_reference", &[a as i64, b as i64], || closures::by_reference(a, b));
            case("closures.add_both", &[a as i64, b as i64], || closures::add_both(a, b));
            case("closures.pattern_param", &[a as i64, b as i64], || closures::pattern_param(a, b));
        }
    }
    for times in 0..5 {
        case("closures.fresh_copy_each_time", &[times as i64], || closures::fresh_copy_each_time(times));
    }
    for (a, b) in [(7, 2), (0, 5), (u32::MAX, 10), (5, 0)] {
        case("structs.divmod", &[a as i64, b as i64], || structs::divmod(a, b));
        case("structs.divmod_sum", &[a as i64, b as i64], || structs::divmod_sum(a, b) as i64);
    }
}

fn case<T: Json>(name: &str, args: &[i64], f: impl FnOnce() -> T + UnwindSafe) {
    let args: Vec<&dyn Json> = args.iter().map(|a| a as &dyn Json).collect();
    case_with(name, &args, f);
}

fn case_with<T: Json>(name: &str, args: &[&dyn Json], f: impl FnOnce() -> T + UnwindSafe) {
    let args: Vec<String> = args.iter().map(|a| a.json()).collect();
    let args = args.join(",");
    let outcome = match panic::catch_unwind(f) {
        Ok(v) => format!("\"value\":{}", v.json()),
        Err(e) => {
            let msg = e.downcast_ref::<&str>().map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            format!("\"panic\":{msg:?}")
        }
    };
    println!("{{\"fn\":{name:?},\"args\":[{args}],{outcome}}}");
}

trait Json {
    fn json(&self) -> String;
}

impl Json for i64 {
    fn json(&self) -> String {
        self.to_string()
    }
}

impl Json for i32 {
    fn json(&self) -> String {
        self.to_string()
    }
}

impl Json for u32 {
    fn json(&self) -> String {
        self.to_string()
    }
}

impl<A: Json, B: Json> Json for (A, B) {
    fn json(&self) -> String {
        format!("[{},{}]", self.0.json(), self.1.json())
    }
}

impl Json for Point {
    fn json(&self) -> String {
        format!("{{\"x\":{},\"y\":{}}}", self.x, self.y)
    }
}

impl Json for Size {
    fn json(&self) -> String {
        format!("[{},{}]", self.0, self.1)
    }
}

impl Json for Rect {
    fn json(&self) -> String {
        format!("{{\"origin\":{},\"size\":{}}}", self.origin.json(), self.size.json())
    }
}
