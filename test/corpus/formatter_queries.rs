// A writer can ask its `Formatter` what the placeholder gave it (ADR 0143):
// `f.width()`, `f.precision()`, `f.fill()`, `f.align()`, `f.sign_plus()`
// and `f.sign_aware_zero_pad()` are its options object's, and `f.pad(s)`
// applies them to a string, as a `str`'s `Display` does.
use std::fmt;

struct Tag(&'static str);

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(self.0)
    }
}

struct Probe;

impl fmt::Display for Probe {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let align = match f.align() {
            Some(fmt::Alignment::Left) => "left",
            Some(fmt::Alignment::Right) => "right",
            Some(fmt::Alignment::Center) => "center",
            None => "none",
        };
        write!(
            f,
            "w={:?} p={:?} fill={:?} align={} plus={} zero={} alt={}",
            f.width(),
            f.precision(),
            f.fill(),
            align,
            f.sign_plus(),
            f.sign_aware_zero_pad(),
            f.alternate()
        )
    }
}

struct Money(i64);

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = format!("${}.{:02}", self.0 / 100, self.0 % 100);
        match f.width() {
            Some(w) => write!(f, "{:>w$}", s, w = w),
            None => f.write_str(&s),
        }
    }
}

fn main() {
    println!("[{:>6}] [{:-<7}] [{:^7.2}] [{}]", Tag("ab"), Tag("cd"), Tag("xyz"), Tag("e"));
    println!("{}", Probe);
    println!("{:+#08.3}", Probe);
    println!("{:*^5}", Probe);
    println!("{:<5}", Probe);
    println!("[{:10}] [{}]", Money(1234), Money(5));
}
