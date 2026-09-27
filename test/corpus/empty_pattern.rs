// An empty pattern matches at each char's boundary, both ends too, where JS
// matches between UTF-16 units: an emoji is one char, two units. Found by
// seed 79.
fn show(text: &str, pattern: &str) {
    println!("{:?} {:?}", text.replace(pattern, "-"), text.split(pattern).collect::<Vec<_>>());
}

fn main() {
    for text in ["🦀x", "ab", "", "é"] {
        show(text, "");
    }
    show("a,b", ",");
    println!("{}", "x🦀y".replace("", "|"));
}
