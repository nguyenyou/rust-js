/// Calls back up into the crate root: a cycle (lib ↔ stats), which Rust allows.
pub fn mean(a: u32, b: u32) -> u32 {
    half(crate::clamp(a) + crate::clamp(b))
}

/// Private and only used here: stays unexported.
fn half(x: u32) -> u32 {
    x / 2
}
