import { expect, test } from "bun:test";
import { mkdirSync, writeFileSync, readFileSync, unlinkSync, chmodSync, existsSync } from "node:fs";
import { join } from "node:path";
import { createLogger, createServer, build } from "vite";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import babel from "@rolldown/plugin-babel";
import tailwindcss from "@tailwindcss/vite";
import { chromium } from "@playwright/test";
import rustJs from "../vite-plugin/index.js";
import { buildCompiler, buildReact, compiler, fixture, root } from "./support";

// Real Vite, real compiler, real React Fast Refresh. Isolated sources and
// outputs: the example application and its development server are untouched.
test("Vite builds and refreshes affected crates, recovers from errors and module changes", async () => {
  buildCompiler();
  const dir = fixture("vite");
  mkdirSync(join(dir, "src"));
  mkdirSync(join(dir, "other"));
  writeFileSync(join(dir, "index.html"), '<div id="root"></div><script type="module" src="/src/main.jsx"></script>');
  writeFileSync(join(dir, "src/main.jsx"), 'import {createRoot} from "react-dom/client"; import {App} from "./App.jsx"; createRoot(document.getElementById("root")).render(<App/>);');
  const source = `#![allow(non_snake_case)]
use react::{Element, jsx, use_state};
mod text;
#[derive(serde::Deserialize)]
pub struct Saved { pub count: i32 }
pub fn App() -> Element {
    let saved: Saved = serde_json::from_str(r#"{"count":0}"#).unwrap();
    let (count, set_count) = use_state(saved.count);
    jsx! { <button onClick={move |_| set_count.update(|n| n + 1)}>{text::label()}{count}</button> }
}
`;
  const app = join(dir, "src/App.rs"), text = join(dir, "src/text.rs"), output = join(dir, "src/App.jsx");
  writeFileSync(app, source);
  writeFileSync(text, 'pub fn label() -> &\'static str { "Count " }');
  writeFileSync(join(dir, "other/lib.rs"), 'pub fn value() -> i32 { 1 }');
  const log = join(dir, "compilations.jsonl"), wrapper = join(dir, "compiler");
  writeFileSync(wrapper, `#!/usr/bin/env bun
import {appendFileSync, writeFileSync, unlinkSync} from "node:fs";
const lock = ${JSON.stringify(join(dir, "compiler.lock"))};
writeFileSync(lock, "running", {flag: "wx"});
appendFileSync(${JSON.stringify(log)}, JSON.stringify(Bun.argv.slice(2)) + "\\n");
await Bun.sleep(75);
const p = Bun.spawn([${JSON.stringify(compiler)}, ...Bun.argv.slice(2)], {stdout: "inherit", stderr: "inherit"});
const code = await p.exited;
unlinkSync(lock);
process.exit(code);
`);
  chmodSync(wrapper, 0o755);
  const plugins = () => [rustJs({ crates: ["src/App.rs", "other/lib.rs"], rustJs: wrapper, bindings: ["react", "serde"] }), react()];
  await build({ root: dir, configFile: false, plugins: plugins(), logLevel: "silent" });
  expect(readFileSync(join(dir, "dist/index.html"), "utf8")).toContain("/assets/");
  const server = await createServer({ root: dir, configFile: false, plugins: plugins(), logLevel: "silent", server: { port: 0 } });
  let browser;
  try {
    await server.listen();
    if (!server.resolvedUrls) throw new Error("Vite did not expose a listening URL");
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    await page.goto(server.resolvedUrls.local[0]);
    const button = page.getByRole("button");
    await button.filter({ hasText: "Count 0" }).waitFor();
    await button.click();
    await button.filter({ hasText: "Count 1" }).waitFor();
    const calls = () => readFileSync(log, "utf8").trim().split("\n").map(line => JSON.parse(line)[0]);
    const before = calls().length;
    writeFileSync(text, 'pub fn label() -> &\'static str { "Changed " }');
    await button.filter({ hasText: "Changed 1" }).waitFor();
    expect(calls().slice(before)).toEqual(["src/App.rs"]);

    writeFileSync(app, source.replace("<button onClick", '<button className="updated" onClick'));
    await page.locator("button.updated").waitFor();
    expect(await button.textContent()).toBe("Changed 1");

    const good = readFileSync(output, "utf8");
    writeFileSync(app, source + "pub fn broken(");
    await page.locator("vite-error-overlay").waitFor();
    expect(readFileSync(output, "utf8")).toBe(good);
    writeFileSync(app, source);
    await page.locator("vite-error-overlay").waitFor({ state: "detached" });
    await button.filter({ hasText: "Changed 1" }).waitFor();

    unlinkSync(text);
    await page.locator("vite-error-overlay").waitFor();
    writeFileSync(text, 'pub fn label() -> &\'static str { "Restored " }');
    await page.locator("vite-error-overlay").waitFor({ state: "detached" });
    await button.filter({ hasText: "Restored 1" }).waitFor();

    // Adding a declared module and quickly editing it must converge on the
    // latest source, without concurrent compiler writes.
    writeFileSync(app, source.replace("mod text;", "mod text; mod extra;").replace("text::label()", "extra::label()"));
    writeFileSync(join(dir, "src/extra.rs"), 'pub fn label() -> &\'static str { "First " }');
    writeFileSync(join(dir, "src/extra.rs"), 'pub fn label() -> &\'static str { "Latest " }');
    await button.filter({ hasText: /Latest [01]/ }).waitFor();
    expect(existsSync(join(dir, "compiler.lock"))).toBe(false);

    // The entry still imports App.jsx: the manifest redirects it when the
    // Rust module stops containing JSX, then restores it on the next edit.
    writeFileSync(app, `#![allow(non_snake_case)]
use react::Element;
#[rust_js::link_name = "react#createElement"]
fn make(tag: &str, props: (), child: &str) -> Element {
    unreachable!()
}
pub fn App() -> Element {
    make("button", (), "Plain")
}
`);
    await button.filter({ hasText: "Plain" }).waitFor();
    expect(existsSync(output)).toBe(false);
    expect(existsSync(join(dir, "src/App.js"))).toBe(true);
    writeFileSync(app, source);
    await button.filter({ hasText: "Restored 0" }).waitFor();
    expect(existsSync(join(dir, "src/App.js"))).toBe(false);
  } finally {
    await browser?.close();
    await server.close();
  }
  writeFileSync(app, "this is not Rust");
  await expect(build({ root: dir, configFile: false, plugins: plugins(), logLevel: "silent" })).rejects.toThrow();
}, 120_000);

// Tailwind reads class names in the Rust, and React Compiler memoizes the
// component rust-js writes (ADR 0041). A save must still be a Fast Refresh:
// Tailwind reloads the page when a file it scans changes, unless that file
// is JS, and a `.rs` file isn't.
test("Tailwind and React Compiler keep Fast Refresh's state", async () => {
  buildReact();
  const dir = fixture("vite-tailwind");
  mkdirSync(join(dir, "src"));
  writeFileSync(join(dir, "index.html"), '<div id="root"></div><script type="module" src="/src/main.jsx"></script>');
  // `target/` is gitignored, which Tailwind's own detection respects.
  writeFileSync(join(dir, "src/index.css"), '@import "tailwindcss";\n@source "./App.rs";\n');
  writeFileSync(join(dir, "src/main.jsx"), 'import "./index.css"; import {createRoot} from "react-dom/client"; import {App} from "./App.jsx"; createRoot(document.getElementById("root")).render(<App/>);');
  const app = (classes: string) => `#![allow(non_snake_case)]
use react::{Element, jsx, use_state};
pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    jsx! {
        <button className="${classes}" onClick={move |_| set_count.update(|n| n + 1)}>
            {"Count "}
            {count}
        </button>
    }
}
`;
  writeFileSync(join(dir, "src/App.rs"), app("font-bold"));
  const plugins = [rustJs({ rustJs: compiler }), react(), babel({ presets: [reactCompilerPreset()] }), tailwindcss()];
  const server = await createServer({ root: dir, configFile: false, plugins, logLevel: "silent", server: { port: 0 } });
  let browser;
  try {
    await server.listen();
    if (!server.resolvedUrls) throw new Error("Vite did not expose a listening URL");
    const url = server.resolvedUrls.local[0];
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    await page.goto(url);
    const button = page.getByRole("button");
    await button.filter({ hasText: "Count 0" }).waitFor();
    expect(await button.evaluate(b => getComputedStyle(b).fontWeight)).toBe("700");
    // React Compiler's memo cache, in the component rust-js wrote.
    expect(await (await fetch(new URL("src/App.jsx", url))).text()).toMatch(/const \$ = _c\(\d+\);/);
    await button.click();
    await button.filter({ hasText: "Count 1" }).waitFor();
    await page.evaluate(() => { (window as any).sameDocument = true; });

    // A class that's new to Tailwind: its CSS arrives, and the state stays.
    writeFileSync(join(dir, "src/App.rs"), app("font-bold underline"));
    await page.locator("button.underline").waitFor();
    await page.waitForFunction(() => getComputedStyle(document.querySelector("button")!).textDecorationLine === "underline");
    expect(await button.textContent()).toBe("Count 1");
    expect(await page.evaluate(() => (window as any).sameDocument)).toBe(true);
  } finally {
    await browser?.close();
    await server.close();
  }
}, 60_000);

// The generated JSX is committed, as ReScript recommends for its JS (ADR 0041),
// and the source map isn't. So a checkout without rust-js still builds: the
// plugin says so and uses the committed file, with or without its map.
test("Without rust-js, a build uses the committed JSX", async () => {
  buildReact();
  const dir = fixture("vite-committed");
  mkdirSync(join(dir, "src"));
  writeFileSync(join(dir, "index.html"), '<div id="root"></div><script type="module" src="/src/main.jsx"></script>');
  writeFileSync(join(dir, "src/main.jsx"), 'import {createRoot} from "react-dom/client"; import {App} from "./App.jsx"; createRoot(document.getElementById("root")).render(<App/>);');
  writeFileSync(join(dir, "src/App.rs"), `#![allow(non_snake_case)]
use react::{Element, jsx};
pub fn App() -> Element {
    jsx! {
        <p>{"Committed"}</p>
    }
}
`);
  await build({ root: dir, configFile: false, plugins: [rustJs({ rustJs: compiler }), react()], logLevel: "silent" });
  const committed = readFileSync(join(dir, "src/App.jsx"), "utf8");
  unlinkSync(join(dir, "src/App.jsx.map"));

  const warnings: string[] = [];
  const logger = { ...createLogger("silent"), warn: (message: string) => { warnings.push(message); } };
  const missing = join(dir, "no-rust-js");
  await build({ root: dir, configFile: false, plugins: [rustJs({ rustJs: missing }), react()], customLogger: logger, logLevel: "warn" });
  expect(readFileSync(join(dir, "src/App.jsx"), "utf8")).toBe(committed);
  expect(readFileSync(join(dir, "dist/index.html"), "utf8")).toContain("/assets/");
  expect(warnings.join("\n")).toContain(`no rust-js at ${missing}: using the committed src/App.jsx`);

  // In dev too, and the map the file names but no one committed isn't an error.
  warnings.length = 0;
  const server = await createServer({ root: dir, configFile: false, plugins: [rustJs({ rustJs: missing }), react()], customLogger: logger, logLevel: "warn", server: { port: 0 } });
  try {
    await server.listen();
    if (!server.resolvedUrls) throw new Error("Vite did not expose a listening URL");
    expect((await server.transformRequest("/src/App.jsx"))?.code).toContain("Committed");
    expect(warnings.join("\n")).toContain("using the committed src/App.jsx");
    expect(warnings.join("\n")).not.toContain("source map");
  } finally {
    await server.close();
  }

  // Without the committed file, there's nothing to fall back on.
  unlinkSync(join(dir, "src/App.jsx"));
  await expect(build({ root: dir, configFile: false, plugins: [rustJs({ rustJs: missing }), react()], logLevel: "silent" })).rejects.toThrow("no rust-js at");
}, 60_000);

// A crate's checks in Vite (ADR 0117): a build runs all of them, the server
// those not only for a build; what one says, of the Rust, is Vite's warning.
test("Vite runs a crate's checks, a build's all, the server's those for a save", async () => {
  buildReact();
  const dir = fixture("vite-checks");
  mkdirSync(join(dir, "src"));
  writeFileSync(join(dir, "index.html"), '<div id="root"></div><script type="module" src="/src/main.jsx"></script>');
  writeFileSync(join(dir, "src/main.jsx"), 'import {createRoot} from "react-dom/client"; import {App} from "./App.jsx"; createRoot(document.getElementById("root")).render(<App/>);');
  writeFileSync(join(dir, "src/App.rs"), `#![allow(non_snake_case)]
use react::{Element, jsx};
pub fn App() -> Element {
    jsx! {
        <p>{"Checked"}</p>
    }
}
`);
  writeFileSync(join(dir, "Cargo.toml"), `[package]
name = "app"
version = "0.0.0"
edition = "2024"

[package.metadata.rust-js.hooks]
check = [
  { run = ["sh", "-c", "touch built"], when = "build" },
  { run = ["sh", "-c", "echo \\"$1:4:3: a note\\"", "sh", "{files}"], fail = "never" },
]
`);
  const warnings: string[] = [];
  const logger = { ...createLogger("silent"), warn: (message: string) => { warnings.push(message); } };
  await build({ root: dir, configFile: false, plugins: [rustJs({ rustJs: compiler }), react()], customLogger: logger, logLevel: "warn" });
  expect(existsSync(join(dir, "built"))).toBe(true);
  expect(warnings.join("\n")).toMatch(/src\/App\.rs:\d+:\d+: \[sh\] a note/);

  unlinkSync(join(dir, "built"));
  warnings.length = 0;
  const server = await createServer({ root: dir, configFile: false, plugins: [rustJs({ rustJs: compiler }), react()], customLogger: logger, logLevel: "warn", server: { port: 0 } });
  try {
    await server.listen();
    expect((await server.transformRequest("/src/App.jsx"))?.code).toContain("Checked");
    expect(existsSync(join(dir, "built"))).toBe(false);
    expect(warnings.join("\n")).toMatch(/src\/App\.rs:\d+:\d+: \[sh\] a note/);
  } finally {
    await server.close();
  }
}, 120_000);

// Fast Refresh keeps state under a context and in a memoized component
// (ADR 0041). Saving a module runs it again, so a context made in it is a new
// one, and React remounts what's under its provider: hand-written React does
// the same. A context in a module of its own, as React advises, is untouched
// when a component's module changes: rust-js leaves unchanged files alone.
test("Fast Refresh keeps state under a context from its own module, and in memo", async () => {
  buildReact();
  const dir = fixture("vite-context");
  mkdirSync(join(dir, "src"));
  writeFileSync(join(dir, "index.html"), '<div id="root"></div><script type="module" src="/src/main.jsx"></script>');
  writeFileSync(join(dir, "src/main.jsx"), 'import {createRoot} from "react-dom/client"; import {App} from "./App.jsx"; createRoot(document.getElementById("root")).render(<App/>);');
  writeFileSync(join(dir, "src/theme.rs"), `use react::{Context, create_context};
thread_local! {
    pub static THEME: Context<&'static str> = create_context("light");
}
`);
  const app = (label: string) => `#![allow(non_snake_case)]
use react::{Element, Memo, jsx, memo, use_context, use_state};
mod theme;
use theme::THEME;
thread_local! {
    static FAST_LABEL: Memo<LabelProps> = memo(Label);
}
pub struct LabelProps {
    pub text: &'static str,
}
pub fn Label(LabelProps { text }: LabelProps) -> Element {
    let theme = use_context(&THEME);
    let (n, set_n) = use_state(0);
    jsx! {
        <button className={*theme} onClick={move |_| set_n.update(|n| n + 1)}>
            {text}
            {" "}
            {n}
        </button>
    }
}
pub fn App() -> Element {
    jsx! {
        <THEME value="dark">
            <FAST_LABEL text="${label}" />
        </THEME>
    }
}
`;
  writeFileSync(join(dir, "src/App.rs"), app("Count"));
  const server = await createServer({ root: dir, configFile: false, plugins: [rustJs({ rustJs: compiler }), react()], logLevel: "silent", server: { port: 0 } });
  let browser;
  try {
    await server.listen();
    if (!server.resolvedUrls) throw new Error("Vite did not expose a listening URL");
    const theme = readFileSync(join(dir, "src/theme.js"), "utf8");
    expect(theme).toContain('export const THEME = createContext("light");');
    expect(readFileSync(join(dir, "src/App.jsx"), "utf8")).toContain('<THEME value="dark">');
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    await page.goto(server.resolvedUrls.local[0]);
    const button = page.getByRole("button");
    await button.filter({ hasText: "Count 0" }).waitFor();
    await button.click();
    await button.filter({ hasText: "Count 1" }).waitFor();
    await page.evaluate(() => { (window as any).sameDocument = true; });

    writeFileSync(join(dir, "src/App.rs"), app("Clicks"));
    await button.filter({ hasText: /^Clicks/ }).waitFor();
    // The state, the context's value, and the page are all still there.
    expect(await button.textContent()).toBe("Clicks 1");
    expect(await button.getAttribute("class")).toBe("dark");
    expect(await page.evaluate(() => (window as any).sameDocument)).toBe(true);
    expect(readFileSync(join(dir, "src/theme.js"), "utf8")).toBe(theme);
  } finally {
    await browser?.close();
    await server.close();
  }
}, 60_000);

// A Cargo workspace (ADR 0101): Cargo builds each crate, rust-js as its
// workspace wrapper, and the app imports its package's JS, where Cargo's
// build of it has it, as `rust-js:<package>`. An edit to a crate the
// component uses is a Fast Refresh, and an error is the overlay. The app is
// a directory of the workspace, as a full-stack one's is: what Cargo builds
// is outside Vite's root. Found in review: a package the Rust imports is the
// app's, in `web/node_modules`, and Vite is given `ui`'s manifest, a member's,
// whose sibling `models` is the workspace's too.
test("Vite builds a Cargo workspace's package and refreshes it when a crate it uses changes", async () => {
  buildCompiler();
  const dir = fixture("vite-cargo");
  writeFileSync(join(dir, "Cargo.toml"), '[workspace]\nmembers = ["ui", "models"]\nresolver = "2"\n');
  for (const name of ["ui", "models"]) mkdirSync(join(dir, name, "src"), { recursive: true });
  writeFileSync(join(dir, "models/Cargo.toml"), '[package]\nname = "models"\nversion = "0.1.0"\nedition = "2024"\n');
  const models = join(dir, "models/src/lib.rs");
  writeFileSync(models, 'pub fn label() -> &\'static str {\n    "Count "\n}\n');
  writeFileSync(join(dir, "ui/Cargo.toml"), `[package]\nname = "ui"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nmodels = { path = "../models" }\nreact = { package = "rust-js-react", path = ${JSON.stringify(join(root, "react"))} }\n`);
  const ui = join(dir, "ui/src/lib.rs");
  const source = `#![allow(non_snake_case)]
use react::{Element, jsx, use_state};

#[rust_js::link_name = "widget#greeting"]
fn greeting() -> &'static str {
    unreachable!()
}

pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    jsx! { <button title={greeting()} onClick={move |_| set_count.update(|n| n + 1)}>{models::label()}{count}</button> }
}
`;
  writeFileSync(ui, source);
  const web = join(dir, "web");
  mkdirSync(join(web, "src"), { recursive: true });
  mkdirSync(join(web, "node_modules/widget"), { recursive: true });
  writeFileSync(join(web, "node_modules/widget/package.json"), '{ "name": "widget", "type": "module", "main": "index.js" }\n');
  writeFileSync(join(web, "node_modules/widget/index.js"), 'export function greeting() { return "hello from widget"; }\n');
  writeFileSync(join(web, "index.html"), '<div id="root"></div><script type="module" src="/src/main.jsx"></script>');
  writeFileSync(join(web, "src/main.jsx"), 'import {createRoot} from "react-dom/client"; import {App} from "rust-js:ui"; createRoot(document.getElementById("root")).render(<App/>);');
  const plugins = () => [rustJs({ rustJs: compiler, cargo: { package: "ui", manifestPath: "../ui/Cargo.toml", offline: true } }), react()];
  await build({ root: web, configFile: false, plugins: plugins(), logLevel: "silent" });
  expect(readFileSync(join(web, "dist/index.html"), "utf8")).toContain("/assets/");
  // The JS is beside the Rust too, as a project commits it (ADR 0041), each
  // crate's importing the others' there.
  expect(readFileSync(join(dir, "ui/src/lib.jsx"), "utf8")).toContain('from "../../models/src/lib.js"');
  expect(existsSync(join(dir, "models/src/lib.js"))).toBe(true);
  const server = await createServer({ root: web, configFile: false, plugins: plugins(), logLevel: "silent", server: { port: 0 } });
  let browser;
  try {
    await server.listen();
    if (!server.resolvedUrls) throw new Error("Vite did not expose a listening URL");
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    await page.goto(server.resolvedUrls.local[0]);
    const button = page.getByRole("button");
    await button.filter({ hasText: "Count 0" }).waitFor();
    expect(await button.getAttribute("title")).toBe("hello from widget");
    await button.click();
    await button.filter({ hasText: "Count 1" }).waitFor();
    await page.evaluate(() => { (window as any).sameDocument = true; });

    writeFileSync(models, 'pub fn label() -> &\'static str {\n    "Changed "\n}\n');
    await button.filter({ hasText: "Changed 1" }).waitFor();
    expect(await page.evaluate(() => (window as any).sameDocument)).toBe(true);

    // A source outside the workspace, a module by `#[path]`, is one rust-js
    // read: an edit of it is one too. Found in review.
    const shared = `${dir}-shared.rs`;
    writeFileSync(shared, 'pub fn label() -> &\'static str {\n    "Shared "\n}\n');
    writeFileSync(models, `#[path = ${JSON.stringify(shared)}]\nmod shared;\n\npub fn label() -> &'static str {\n    shared::label()\n}\n`);
    // Another module is another set of files: a reload.
    await button.filter({ hasText: "Shared 0" }).waitFor();
    await button.click();
    writeFileSync(shared, 'pub fn label() -> &\'static str {\n    "Edited "\n}\n');
    await button.filter({ hasText: "Edited 1" }).waitFor();

    writeFileSync(ui, source + "pub fn broken(");
    await page.locator("vite-error-overlay").waitFor();
    writeFileSync(ui, source);
    await page.locator("vite-error-overlay").waitFor({ state: "detached" });
    await button.filter({ hasText: "Edited 1" }).waitFor();
  } finally {
    await browser?.close();
    await server.close();
  }
  expect(() => rustJs({ cargo: { package: "ui" }, compile: async () => {} })).toThrow("give `cargo` or `compile`, not both");
  // Without rust-js, a build is of the JS committed beside the Rust.
  const logger = createLogger("warn", { allowClearScreen: false });
  const warnings: string[] = [];
  logger.warn = (message) => { warnings.push(message); };
  const missing = [rustJs({ rustJs: join(dir, "no-rust-js"), cargo: { package: "ui", manifestPath: "../ui/Cargo.toml", offline: true } }), react()];
  await build({ root: web, configFile: false, plugins: missing, customLogger: logger, logLevel: "warn" });
  expect(warnings.join("\n")).toContain("using the committed");
  expect(readFileSync(join(web, "dist/index.html"), "utf8")).toContain("/assets/");
  writeFileSync(ui, source + "pub fn broken(");
  await expect(build({ root: web, configFile: false, plugins: plugins(), logLevel: "silent" })).rejects.toThrow("pub fn broken(");
}, 180_000);
