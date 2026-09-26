// The line beside the buttons: what the page is doing, or how it went.

use react::Element;
use react::html::span;

pub enum Tone {
    Plain,
    Good,
    Bad,
}

pub struct Status {
    pub text: String,
    pub tone: Tone,
}

pub struct StatusLineProps {
    pub status: &'static Status,
}

pub fn StatusLine(StatusLineProps { status }: StatusLineProps) -> Element {
    let color = match status.tone {
        Tone::Plain => "",
        Tone::Good => "text-good",
        Tone::Bad => "text-bad",
    };
    span().id("status").role("status").class_name(color).children(status.text.clone())
}
