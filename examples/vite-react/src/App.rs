// create-vite's React template, `App.jsx`, written in Rust. rust-js compiles
// it to `App.jsx` beside it (@rust-js/vite-plugin does, on every save), and
// Vite serves that with Fast Refresh, as it would the original.

#![allow(non_snake_case)]

js::import!("./App.css");

use react::{Element, jsx, use_state};

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

    jsx! {
        <>
            <section id="center">
                <div className="hero">
                    <img src={hero_img} className="base" width="170" height="179" alt="" />
                    <img src={react_logo} className="framework" alt="React logo" />
                    <img src={vite_logo} className="vite" alt="Vite logo" />
                </div>
                <div>
                    <h1>{"Get started"}</h1>
                    <p>
                        {"Edit "}
                        <code>{"src/App.rs"}</code>
                        {" and save to test "}
                        <code>{"HMR"}</code>
                    </p>
                </div>
                <button type="button" className="counter" onClick={move |_| set_count.update(|count| count + 1)}>
                    {"Count is "}
                    {/* Tailwind finds its classes in this file: whole string literals. */}
                    <span
                        className={if count % 2 == 0 { "font-bold text-emerald-500" } else { "font-bold text-sky-500" }}
                    >
                        {count}
                    </span>
                </button>
            </section>
            <div className="ticks" />
            <section id="next-steps">
                <div id="docs">
                    <svg className="icon" role="presentation" aria-hidden="true">
                        <use href="/icons.svg#documentation-icon" />
                    </svg>
                    <h2>{"Documentation"}</h2>
                    <p>{"Your questions, answered"}</p>
                    <ul>
                        <li>
                            <a href="https://vite.dev/" target="_blank">
                                <img className="logo" src={vite_logo} alt="" />
                                {"Explore Vite"}
                            </a>
                        </li>
                        <li>
                            <a href="https://react.dev/" target="_blank">
                                <img className="button-icon" src={react_logo} alt="" />
                                {"Learn more"}
                            </a>
                        </li>
                    </ul>
                </div>
                <div id="social">
                    <svg className="icon" role="presentation" aria-hidden="true">
                        <use href="/icons.svg#social-icon" />
                    </svg>
                    <h2>{"Connect with us"}</h2>
                    <p>{"Join the Vite community"}</p>
                    <ul>
                        <li>
                            <a href="https://github.com/vitejs/vite" target="_blank">
                                <svg className="button-icon" role="presentation" aria-hidden="true">
                                    <use href="/icons.svg#github-icon" />
                                </svg>
                                {"GitHub"}
                            </a>
                        </li>
                        <li>
                            <a href="https://chat.vite.dev/" target="_blank">
                                <svg className="button-icon" role="presentation" aria-hidden="true">
                                    <use href="/icons.svg#discord-icon" />
                                </svg>
                                {"Discord"}
                            </a>
                        </li>
                        <li>
                            <a href="https://x.com/vite_js" target="_blank">
                                <svg className="button-icon" role="presentation" aria-hidden="true">
                                    <use href="/icons.svg#x-icon" />
                                </svg>
                                {"X.com"}
                            </a>
                        </li>
                        <li>
                            <a href="https://bsky.app/profile/vite.dev" target="_blank">
                                <svg className="button-icon" role="presentation" aria-hidden="true">
                                    <use href="/icons.svg#bluesky-icon" />
                                </svg>
                                {"Bluesky"}
                            </a>
                        </li>
                    </ul>
                </div>
            </section>
            <div className="ticks" />
            <section id="spacer" />
        </>
    }
}
