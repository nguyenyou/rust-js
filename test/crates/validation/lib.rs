//! The crate at the bottom of the graph (ADR 0100): free functions of
//! strings and numbers, which `models` calls, and `frontend` too.

pub fn email(address: &str) -> Result<(), String> {
    if address.contains('@') {
        Ok(())
    } else {
        Err(format!("`{address}` isn't an email address"))
    }
}

pub fn between(n: u32, low: u32, high: u32) -> bool {
    low <= n && n <= high
}
