// A chain of types, each holding the one before four times: walked once
// each, not 4^20 times. rust-js checked each path through them again, and
// never finished. Found by rustc's `issue-42747`.
macro_rules! chain {
    ($cur:ident $prev:ty) => {
        #[derive(Debug, Clone)]
        enum $cur {
            Empty,
            First($prev),
            Second($prev),
            Third($prev),
            Fourth($prev),
        }
    };
}

chain!(C0 ());
chain!(C1 C0);
chain!(C2 C1);
chain!(C3 C2);
chain!(C4 C3);
chain!(C5 C4);
chain!(C6 C5);
chain!(C7 C6);
chain!(C8 C7);
chain!(C9 C8);
chain!(C10 C9);
chain!(C11 C10);
chain!(C12 C11);
chain!(C13 C12);
chain!(C14 C13);
chain!(C15 C14);
chain!(C16 C15);
chain!(C17 C16);
chain!(C18 C17);
chain!(C19 C18);

fn main() {
    let deep = C19::Second(C18::Empty);
    let top = C2::Third(C1::First(C0::Fourth(())));
    println!("{deep:?} {top:?} {}", matches!(deep.clone(), C19::Second(_)));
}
