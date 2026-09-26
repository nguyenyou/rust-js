//! Runs the examples natively and prints every result as JSON lines. The
//! JS test runs the same calls on the generated JS and compares.
//!
//! Values are printed the way ADR 0020 says JS holds them: a struct as an
//! object, a tuple (or tuple struct) as an array.

#[path = "../examples/fib.rs"]
#[allow(dead_code)]
mod fib;

#[path = "../examples/collections.rs"]
#[allow(dead_code)]
mod collections;

#[path = "../examples/closures.rs"]
#[allow(dead_code)]
mod closures;

#[path = "../examples/structs.rs"]
#[allow(dead_code)]
mod structs;

#[path = "../examples/options.rs"]
#[allow(dead_code)]
mod options;

#[path = "../examples/consts.rs"]
#[allow(dead_code)]
mod consts;

#[path = "../examples/enums.rs"]
#[allow(dead_code)]
mod enums;

#[path = "../examples/strings.rs"]
#[allow(dead_code)]
mod strings;

// `modules`: examples/modules/lib.rs, a crate split across files, linked
// with `--extern`. (It can't be pulled in with `#[path]` like fib.rs: its
// `crate::` paths must mean its own root.)

use std::panic::{self, UnwindSafe};

use fib::*;
use options::Slot;
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
    for n in [0, 1, 2, 5, 9, 12] {
        case("collections.sum_to", &[n], || collections::sum_to(n as u32));
        case("collections.end_once", &[n], || collections::end_once(n as i32));
        case("collections.mut_counter", &[n], || collections::mut_counter(n as i32));
        case("collections.evens", &[n], || collections::evens(n as u32));
        case("collections.keep_over", &[n], || collections::keep_over(n as i32));
        case("collections.lengths", &[n], || collections::lengths(n as u32));
        case("collections.iterate", &[n], || collections::iterate(n as u32));
        case("collections.labeled", &[n], || collections::labeled(n as u32));
        case("collections.toggled", &[n], || collections::toggled(n as u32));
        case("collections.cell", &[n], || collections::cell(n as i32));
        case("collections.shared", &[n], || collections::shared(n as i32));
    }
    for s in ["", "  ", "hi", "  hi  ", "hello"] {
        case_with("collections.words", &[&s], || collections::words(s));
    }
    for n in [-6, -3, 0, 1, 2, 7, 8, 64, 96] {
        case("options.half", &[n], || options::half(n as i32));
        case("options.half_or_zero", &[n], || options::half_or_zero(n as i32));
        case("options.halvings", &[n], || options::halvings(n as i32));
        case("options.methods", &[n], || options::methods(n as i32));
        case("options.unwrapped", &[n], || options::unwrapped(n as i32));
        case("options.expected", &[n], || options::expected(n as i32));
        case("options.eager", &[n], || options::eager(n as i32));
        case("options.label", &[n], || options::label(n as i32));
        let slot = Slot { id: 1, value: Some(5) };
        case_with("options.fill", &[&slot, &(n as i64)], || options::fill(slot, n as i32));
    }
    for n in [0, 1, 3] {
        case_with("strings.labeled", &[&"box", &(n as i64)], || strings::labeled("box", n));
        case("strings.built", &[n as i64], || strings::built(n));
        case_with("strings.repeated", &[&"ab", &(n as i64)], || strings::repeated("ab", n));
    }
    for s in ["", "abc", "ab/c", "/a//b/", "  Mixed Case  ", "src/geometry.rs", "stats.rs", "äbc/Ö"] {
        case_with("strings.tests", &[&s], || strings::tests(s));
        case_with("strings.cases", &[&s], || strings::cases(s));
        case_with("strings.trimmed", &[&s], || strings::trimmed(s));
        case_with("strings.replaced", &[&s], || strings::replaced(s));
        case_with("strings.module_name", &[&s], || strings::module_name(s));
        case_with("strings.parts", &[&s], || strings::parts(s));
        case_with("strings.rejoined", &[&s], || strings::rejoined(s));
    }
    for windows in [false, true] {
        case_with("strings.separator", &[&windows], || strings::separator(windows));
    }
    let shapes = [enums::Shape::Empty, enums::Shape::Circle(2), enums::Shape::Circle(11), enums::Shape::Rect { w: 0, h: 5 }, enums::Shape::Rect { w: 2, h: 3 }];
    for s in shapes {
        case_with("enums.area", &[&s], || enums::area(s));
        case_with("enums.classify", &[&s], || enums::classify(s));
        case_with("enums.is_round", &[&s], || enums::is_round(s));
        case_with("enums.width", &[&s], || enums::width(s));
        for t in shapes {
            case_with("enums.same", &[&s, &t], || enums::same(s, t));
        }
    }
    case("enums.circle", &[4], || enums::circle(4));
    case("enums.rect", &[2, 3], || enums::rect(2, 3));
    case("enums.empty", &[], enums::empty);
    for depth in 0..5 {
        case("enums.tree_sum", &[depth], || enums::tree_sum(depth as u32));
    }
    for (a, b) in [(7, 2), (1, 0), (-9, 3)] {
        case("enums.checked_div", &[a, b], || enums::checked_div(a as i32, b as i32));
        case("enums.div_or", &[a, b, -1], || enums::div_or(a as i32, b as i32, -1));
    }
    case("consts.size_in_kb", &[], consts::size_in_kb);
    case("consts.greeting", &[], consts::greeting);
    case("consts.on", &[], consts::on);
    case("consts.pair", &[], consts::pair);
    case("consts.prime_sum", &[], consts::prime_sum);
    case("consts.nothing", &[], consts::nothing);
    case("consts.high", &[], consts::high);
    case("consts.limits", &[], consts::limits);
    case("consts.local", &[], consts::local);
    for dx in [0, 5, -2] {
        case("consts.moved", &[dx], || consts::moved(dx as i32));
    }
    for x in [0, 1, 10] {
        case("consts.quarter", &[x], || consts::quarter(x as f64));
    }
    let some_none = [None, Some(0), Some(-4), Some(3)];
    for o in some_none {
        case_with("options.describe", &[&o], || options::describe(o));
        for p in some_none {
            case_with("options.same", &[&o, &p], || options::same(o, p));
            let (a, b) = (Slot { id: 1, value: o }, Slot { id: 1, value: p });
            case_with("options.same_slots", &[&a, &b], || options::same_slots(a, b));
        }
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

impl Json for u64 {
    fn json(&self) -> String {
        self.to_string()
    }
}

impl Json for usize {
    fn json(&self) -> String {
        self.to_string()
    }
}

impl Json for bool {
    fn json(&self) -> String {
        self.to_string()
    }
}

impl Json for &str {
    fn json(&self) -> String {
        format!("{self:?}")
    }
}

impl Json for String {
    fn json(&self) -> String {
        format!("{self:?}")
    }
}

impl<T: Json> Json for Vec<T> {
    fn json(&self) -> String {
        let items: Vec<String> = self.iter().map(|x| x.json()).collect();
        format!("[{}]", items.join(","))
    }
}

impl<A: Json, B: Json, C: Json, D: Json> Json for (A, B, C, D) {
    fn json(&self) -> String {
        format!("[{},{},{},{}]", self.0.json(), self.1.json(), self.2.json(), self.3.json())
    }
}

impl<A: Json, B: Json, C: Json> Json for (A, B, C) {
    fn json(&self) -> String {
        format!("[{},{},{}]", self.0.json(), self.1.json(), self.2.json())
    }
}

impl<A: Json, B: Json> Json for (A, B) {
    fn json(&self) -> String {
        format!("[{},{}]", self.0.json(), self.1.json())
    }
}

/// `None` is `null` in JSON; the test counts JS's `undefined` as `null` too.
impl<T: Json> Json for Option<T> {
    fn json(&self) -> String {
        match self {
            Some(x) => x.json(),
            None => "null".to_string(),
        }
    }
}

impl Json for Slot {
    fn json(&self) -> String {
        format!("{{\"id\":{},\"value\":{}}}", self.id, self.value.json())
    }
}

/// ReScript's shapes (ADR 0033): a name, or an object tagged with it.
impl Json for enums::Shape {
    fn json(&self) -> String {
        match self {
            enums::Shape::Empty => "\"Empty\"".to_string(),
            enums::Shape::Circle(r) => format!("{{\"TAG\":\"Circle\",\"_0\":{r}}}"),
            enums::Shape::Rect { w, h } => format!("{{\"TAG\":\"Rect\",\"w\":{w},\"h\":{h}}}"),
        }
    }
}

impl<T: Json, E: Json> Json for Result<T, E> {
    fn json(&self) -> String {
        match self {
            Ok(v) => format!("{{\"TAG\":\"Ok\",\"_0\":{}}}", v.json()),
            Err(e) => format!("{{\"TAG\":\"Err\",\"_0\":{}}}", e.json()),
        }
    }
}

impl Json for f64 {
    fn json(&self) -> String {
        self.to_string()
    }
}

impl Json for consts::Point {
    fn json(&self) -> String {
        format!("{{\"x\":{},\"y\":{}}}", self.x, self.y)
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
