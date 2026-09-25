// A crate split across files: one JS file per module (ADR 0019).
//
//   lib.rs            ──► lib.js
//   stats.rs          ──► stats.js
//   geometry.rs       ──► geometry.js
//   geometry/area.rs  ──► geometry/area.js
//   mod util { .. }   ──► util.js   (inline modules get a file too)

mod geometry;
pub mod stats;

/// Calls into two other files.
pub fn summary(a: u32, b: u32) -> u32 {
    stats::mean(a, b) + geometry::area::square(a)
}

/// Private in Rust, but child modules call it, so its JS file must export it.
fn clamp(x: u32) -> u32 {
    if x > 1000 { 1000 } else { x }
}

/// An inline module.
pub mod util {
    pub fn double(x: u32) -> u32 {
        x * 2
    }
}

pub fn doubled_mean(a: u32, b: u32) -> u32 {
    util::double(stats::mean(a, b))
}

/// Reaches two levels down, where both `util` modules are used.
pub fn mixed(x: u32) -> u32 {
    geometry::area::mixed(x)
}

pub fn shadowed(x: u32) -> u32 {
    geometry::area::shadowed(x)
}
