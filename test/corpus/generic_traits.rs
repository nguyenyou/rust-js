// A trait with type parameters, `Convert<T>` (ADR 0106): each impl of it is
// a dictionary of its own, named for its arguments too, `metersConvertF64`
// and `metersConvertString`, and a function bounded by two is given two.
// Its `Option<T>` is what ADR 0051's is: boxed only where the payload could
// look like `None`, so a concrete impl's and a generic one's agree.
trait Convert<T> {
    fn convert(&self) -> T;
    fn twice(&self) -> (T, T) {
        (self.convert(), self.convert())
    }
}

struct Meters(f64);

impl Convert<f64> for Meters {
    fn convert(&self) -> f64 {
        self.0
    }
}

impl Convert<String> for Meters {
    fn convert(&self) -> String {
        format!("{} m", self.0)
    }
}

fn show<X: Convert<String>>(x: &X) -> String {
    x.convert()
}

fn both<X: Convert<f64> + Convert<String>>(x: &X) -> (f64, String) {
    (x.convert(), x.convert())
}

trait Take<T> {
    fn take(&mut self) -> Option<T>;
}

struct Nums {
    list: Vec<u32>,
}

impl Take<u32> for Nums {
    fn take(&mut self) -> Option<u32> {
        self.list.pop()
    }
}

struct Stack<T> {
    list: Vec<T>,
}

impl<T> Take<T> for Stack<T> {
    fn take(&mut self) -> Option<T> {
        self.list.pop()
    }
}

fn first<X: Take<T>, T>(x: &mut X) -> Option<T> {
    x.take()
}

fn count<X: Take<T>, T>(x: &mut X) -> usize {
    let mut n = 0;
    while x.take().is_some() {
        n += 1;
    }
    n
}

// Supertraits: one generic trait twice, each its own key in the dictionary,
// `LabelU32` and `LabelString`, named as the trait declares them, so a
// generic one's, `Pair<A, B>: Label<A> + Label<B>`, are `LabelA` and `LabelB`
// for its impl and its caller alike; one with the trait's own parameter,
// `Wrap<T>: Convert<T>`, just `Convert`; and a higher-ranked
// one, `for<'a> Greet<&'a str>`, one dictionary, its lifetime erased. Found
// by rustc's tests: the two keys collided, and the higher-ranked one crashed.
trait Label<T> {
    fn label(&self) -> T;
}

impl Label<u32> for Meters {
    fn label(&self) -> u32 {
        self.0 as u32
    }
}

impl Label<String> for Meters {
    fn label(&self) -> String {
        "meters".to_string()
    }
}

trait Both: Label<u32> + Label<String> {
    fn both(&self) -> (u32, String) {
        (self.label(), self.label())
    }
}

impl Both for Meters {}

trait Pair<A, B>: Label<A> + Label<B> {}

impl Pair<u32, String> for Meters {}

fn labels<P: Pair<A, B>, A, B>(p: &P) -> (A, B) {
    (<P as Label<A>>::label(p), <P as Label<B>>::label(p))
}

fn number<B: Both>(b: &B) -> u32 {
    b.label()
}

trait Wrap<T>: Convert<T> {
    fn wrapped(&self) -> Vec<T> {
        vec![self.convert()]
    }
}

impl Wrap<f64> for Meters {}

fn unwrapped<X: Wrap<T>, T>(x: &X) -> T {
    x.convert()
}

trait Greet<T> {
    fn greet(&self) -> String {
        "hi".to_string()
    }
}

impl<'a> Greet<&'a str> for Meters {}

trait Greeter: for<'a> Greet<&'a str> {}

impl Greeter for Meters {}

fn main() {
    let m = Meters(2.5);
    let a: f64 = m.convert();
    let b: String = m.convert();
    let pair: (String, String) = m.twice();
    println!("{a} {b} {} {:?} {:?}", show(&m), both(&m), pair);

    let mut nums = Nums { list: vec![1, 2] };
    let mut stack = Stack { list: vec![3u32, 4] };
    println!("{:?} {:?} {:?} {:?}", first(&mut nums), nums.take(), first(&mut stack), stack.take());
    // `None`s in a `Stack`: generic code tells a taken `None` from none left.
    println!("{} {}", count(&mut Stack { list: vec![None::<u32>, None, Some(1)] }), count(&mut Nums { list: vec![7, 8] }));

    let d: &dyn Convert<String> = &m;
    println!("{}", d.convert());

    let x: f64 = unwrapped(&m);
    println!("{:?} {} {:?} {x} {:?}", m.both(), number(&m), m.wrapped(), labels(&m));
    let g: &dyn Greeter = &m;
    println!("{}", g.greet());
}
