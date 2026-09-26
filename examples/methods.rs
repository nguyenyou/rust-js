// Methods: an `impl` block's functions are its type's, in JS an object of
// them named after the type (ADR 0047). `counter.tick()` is
// `Counter.tick(counter)`.

pub struct Counter {
    pub count: u32,
    pub step: u32,
}

impl Counter {
    pub fn new(step: u32) -> Counter {
        Counter { count: 0, step }
    }

    pub fn default_step() -> u32 {
        1
    }

    /// `&mut self`: the object itself changes.
    pub fn tick(&mut self) {
        self.count += self.step;
    }

    /// `&self`, and `Self` for the type.
    pub fn ticked(&self, times: u32) -> Self {
        let mut next = Self { count: self.count, step: self.step };
        for _ in 0..times {
            next.tick();
        }
        next
    }

    pub fn value(&self) -> u32 {
        self.count
    }
}

pub enum Light {
    Red,
    Green,
}

impl Light {
    /// `self` by value.
    pub fn next(self) -> Light {
        match self {
            Light::Red => Light::Green,
            Light::Green => Light::Red,
        }
    }

    pub fn is_go(&self) -> bool {
        matches!(self, Light::Green)
    }
}

/// Another `new`: each type's are its own.
pub struct Pair(pub i32, pub i32);

impl Pair {
    pub fn new(a: i32, b: i32) -> Pair {
        Pair(a, b)
    }

    pub fn sum(&self) -> i32 {
        self.0 + self.1
    }
}

pub fn counted(step: u32, times: u32) -> u32 {
    Counter::new(step).ticked(times).value()
}

pub fn ticking(step: u32) -> u32 {
    let mut counter = Counter::new(step);
    counter.tick();
    counter.tick();
    counter.value()
}

pub fn lights(n: u32) -> bool {
    let mut light = Light::Red;
    for _ in 0..n {
        light = light.next();
    }
    light.is_go()
}

pub fn pair_sum(a: i32, b: i32) -> i32 {
    Pair::new(a, b).sum() + Counter::default_step() as i32
}
