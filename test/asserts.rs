//! What `#[test]` and the assertion macros become (ADR 0026). Some of these
//! tests fail on purpose: test/browser.test.ts checks each failure's message.
#![feature(extern_types)]

#[derive(PartialEq, Debug)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Zero, where rustc can't see it's zero (it rejects `1 / 0` outright).
pub fn zero() -> i32 {
    0
}

unsafe extern "Rust" {
    type Url;

    // Throws a `TypeError` for a bad URL: JS going wrong, not a panic.
    #[link_name = "new URL"]
    safe fn new_url(href: &str) -> &'static Url;

    // Leaves a rejected promise no one handles: a failure that comes later.
    #[link_name = "Promise.reject"]
    safe fn reject_later(reason: &str);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes() {
        assert!(1 + 1 == 2);
        assert_eq!(Point { x: 1, y: 2 }, Point { x: 1, y: 2 });
        assert_ne!(vec![1, 2], vec![2, 1]);
        assert_eq!("a".to_string() + "b", "ab");
    }

    #[test]
    #[should_panic]
    fn panics_as_it_should() {
        panic!("boom");
    }

    #[test]
    #[should_panic(expected = "divide by zero")]
    fn panics_with_the_right_message() {
        let _ = 1 / zero();
    }

    #[test]
    #[ignore]
    fn ignored() {
        panic!("never runs");
    }

    #[test]
    fn fails_an_assert() {
        let n = 3;
        assert!(n < 2, "n was {}", n);
    }

    #[test]
    fn fails_an_assert_eq() {
        assert_eq!(Point { x: 1, y: 2 }, Point { x: 1, y: 3 });
    }

    #[test]
    #[should_panic(expected = "nope")]
    fn panics_with_the_wrong_message() {
        panic!("{:?} happened", "something");
    }

    #[test]
    #[should_panic]
    fn does_not_panic() {}

    #[test]
    #[should_panic]
    fn throws_a_type_error() {
        let _ = new_url("not a url");
    }

    #[test]
    #[should_panic(expected = "Invalid URL")]
    fn throws_a_type_error_with_the_message() {
        let _ = new_url("not a url");
    }

    #[test]
    fn rejects_a_promise() {
        reject_later("rejected later");
    }
}
