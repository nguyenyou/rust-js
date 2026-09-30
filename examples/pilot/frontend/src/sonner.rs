//! [Sonner](https://sonner.emilkowal.ski), an npm package of toasts: its
//! component and its function, bound as any JS is (ADRs 0039, 0040).

// A binding's parameters are its JS function's: its body never runs.
#![allow(unused_variables)]

use react::Element;

pub struct ToasterProps {
    pub position: &'static str,
}

/// Where the toasts show; one per app.
#[cfg_attr(rust_js, rust_js::link_name = "sonner#Toaster")]
pub fn Toaster(props: ToasterProps) -> Element {
    unreachable!()
}

/// A toast with `message`.
#[cfg_attr(rust_js, rust_js::link_name = "sonner#toast")]
pub fn toast(message: &str) {
    unreachable!()
}
