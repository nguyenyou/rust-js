pub mod area;

/// A second module named `util`: where both are imported, one alias gets a suffix.
pub mod util {
    pub fn triple(x: u32) -> u32 {
        x * 3
    }
}
