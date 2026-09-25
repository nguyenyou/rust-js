/// Two levels deep: imports the root as `../lib.js`.
pub fn square(x: u32) -> u32 {
    let c = crate::clamp(x);
    c * c
}

/// Uses `crate::util` and `geometry::util`: both are called `util`.
pub fn mixed(x: u32) -> u32 {
    crate::util::double(x) + super::util::triple(x)
}

/// A local named like an import alias must not shadow the import.
pub fn shadowed(x: u32) -> u32 {
    let util = x + 1;
    crate::util::double(util)
}
