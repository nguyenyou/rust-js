// U+FFFD, the replacement character, printed as its three bytes: what a
// byte that isn't UTF-8 reads as, and which the harness mustn't take one
// for (ADR 0088).
fn main() {
    println!("\u{FFFD}");
    eprintln!("{}", char::REPLACEMENT_CHARACTER);
}
