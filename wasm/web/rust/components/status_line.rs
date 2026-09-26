// The line beside the buttons: what the page is doing, or how it went.

use react::Element;

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
    jsx! { <span id="status" role="status" className={color}>{status.text.clone()}</span> }
}
