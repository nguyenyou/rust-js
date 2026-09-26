// create-vite's React template, `App.jsx`, written in Rust. rust-js compiles
// it to `App.jsx` beside it (vite-plugin-rust-js does, on every save), and
// Vite serves that with Fast Refresh, as it would the original.

#![rust_js::import = "./App.css"]
#![allow(non_snake_case)]

use react::html::{a, button, code, div, h1, h2, img, li, p, section, span, svg, ul, r#use};
use react::{Element, fragment, use_state};

unsafe extern "Rust" {
    #[link_name = "./assets/hero.png#default"]
    safe static hero_img: &'static str;
    #[link_name = "./assets/react.svg#default"]
    safe static react_logo: &'static str;
    #[link_name = "./assets/vite.svg#default"]
    safe static vite_logo: &'static str;
}

pub fn App() -> Element {
    let (count, set_count) = use_state(0);

    fragment((
        section().id("center").children((
            div().class_name("hero").children((
                img().src(hero_img).class_name("base").width("170").height("179").alt(""),
                img().src(react_logo).class_name("framework").alt("React logo"),
                img().src(vite_logo).class_name("vite").alt("Vite logo"),
            )),
            div().children((
                h1().children("Get started"),
                p().children(("Edit ", code().children("src/App.rs"), " and save to test ", code().children("HMR"))),
            )),
            button()
                .r#type("button")
                .class_name("counter")
                .on_click(move |_| set_count.update(|count| count + 1))
                .children((
                    "Count is ",
                    // Tailwind finds its classes in this file: whole string literals.
                    span()
                        .class_name(if count % 2 == 0 { "font-bold text-emerald-500" } else { "font-bold text-sky-500" })
                        .children(count),
                )),
        )),
        div().class_name("ticks"),
        section().id("next-steps").children((
            div().id("docs").children((
                svg()
                    .class_name("icon")
                    .role("presentation")
                    .attr("aria-hidden", "true")
                    .children(r#use().href("/icons.svg#documentation-icon")),
                h2().children("Documentation"),
                p().children("Your questions, answered"),
                ul().children((
                    li().children(a().href("https://vite.dev/").target("_blank").children((
                        img().class_name("logo").src(vite_logo).alt(""),
                        "Explore Vite",
                    ))),
                    li().children(a().href("https://react.dev/").target("_blank").children((
                        img().class_name("button-icon").src(react_logo).alt(""),
                        "Learn more",
                    ))),
                )),
            )),
            div().id("social").children((
                svg()
                    .class_name("icon")
                    .role("presentation")
                    .attr("aria-hidden", "true")
                    .children(r#use().href("/icons.svg#social-icon")),
                h2().children("Connect with us"),
                p().children("Join the Vite community"),
                ul().children((
                    li().children(a().href("https://github.com/vitejs/vite").target("_blank").children((
                        svg()
                            .class_name("button-icon")
                            .role("presentation")
                            .attr("aria-hidden", "true")
                            .children(r#use().href("/icons.svg#github-icon")),
                        "GitHub",
                    ))),
                    li().children(a().href("https://chat.vite.dev/").target("_blank").children((
                        svg()
                            .class_name("button-icon")
                            .role("presentation")
                            .attr("aria-hidden", "true")
                            .children(r#use().href("/icons.svg#discord-icon")),
                        "Discord",
                    ))),
                    li().children(a().href("https://x.com/vite_js").target("_blank").children((
                        svg()
                            .class_name("button-icon")
                            .role("presentation")
                            .attr("aria-hidden", "true")
                            .children(r#use().href("/icons.svg#x-icon")),
                        "X.com",
                    ))),
                    li().children(a().href("https://bsky.app/profile/vite.dev").target("_blank").children((
                        svg()
                            .class_name("button-icon")
                            .role("presentation")
                            .attr("aria-hidden", "true")
                            .children(r#use().href("/icons.svg#bluesky-icon")),
                        "Bluesky",
                    ))),
                )),
            )),
        )),
        div().class_name("ticks"),
        section().id("spacer"),
    ))
}
