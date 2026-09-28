// A `static mut` is its module's `{ value }` (ADR 0096): reads and writes
// are of `.value`, from its own module or another. Each is read by value
// here; a shared reference to one is in static_mut_shared.rs.
#[derive(Clone, Copy, Debug)]
struct Stats {
    calls: u32,
    last: i64,
}

static mut COUNT: u32 = 0;
static mut STATS: Stats = Stats { calls: 0, last: -1 };
static mut SLOTS: [u8; 3] = [0; 3];
static mut WRAP: u8 = 250;

mod log {
    pub static mut LINES: usize = 0;

    pub fn line() -> usize {
        unsafe {
            LINES += 1;
            LINES
        }
    }
}

fn bump() -> u32 {
    unsafe {
        COUNT += 1;
        COUNT
    }
}

fn record(value: i64) {
    unsafe {
        STATS.calls += 1;
        STATS.last = value;
        SLOTS[(value as usize) % 3] = value as u8;
        WRAP = WRAP.wrapping_add(3);
    }
}

fn main() {
    let first = bump();
    let second = bump();
    println!("{} {} {}", first, second, unsafe { COUNT });
    record(4);
    record(8);
    let stats = unsafe { STATS };
    record(1);
    let (calls, last, slot) = unsafe { (STATS.calls, STATS.last, SLOTS[1]) };
    println!("{:?} {} {} {}", stats, calls, last, slot);
    let (slots, wrap) = unsafe { (SLOTS, WRAP) };
    println!("{:?} {}", slots, wrap);
    unsafe {
        log::LINES = 10;
    }
    println!("{} {}", log::line(), log::line());
}
