import { expect, test } from "bun:test";
import { mkdirSync, writeFileSync, readFileSync, unlinkSync, chmodSync, existsSync } from "node:fs";
import { join } from "node:path";
import { createLogger, createServer, build } from "vite";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import babel from "@rolldown/plugin-babel";
import tailwindcss from "@tailwindcss/vite";
import { chromium } from "@playwright/test";
import rustJs from "../vite-plugin/index.js";
import { buildCompiler, buildReact, compiler, fixture } from "./support";

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
use react::{Element, use_state};
mod text;
pub fn App() -> Element {
    let (count, set_count) = use_state(0);
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
  const plugins = () => [rustJs({ crates: ["src/App.rs", "other/lib.rs"], rustJs: wrapper }), react()];
  await build({ root: dir, configFile: false, plugins: plugins(), logLevel: "silent" });
  expect(readFileSync(join(dir, "dist/index.html"), "utf8")).toContain("/assets/");
  const server = await createServer({ root: dir, configFile: false, plugins: plugins(), logLevel: "silent", server: { port: 0 } });
  let browser;
  try {
    await server.listen();
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
fn make(tag: &str, props: (), child: &str) -> Element { unreachable!() }
pub fn App() -> Element { make("button", (), "Plain") }
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
use react::{Element, use_state};
use react::html::button;
pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    button().class_name("${classes}").on_click(move |_| set_count.update(|n| n + 1)).children(("Count ", count))
}
`;
  writeFileSync(join(dir, "src/App.rs"), app("font-bold"));
  const plugins = [rustJs(), react(), babel({ presets: [reactCompilerPreset()] }), tailwindcss()];
  const server = await createServer({ root: dir, configFile: false, plugins, logLevel: "silent", server: { port: 0 } });
  let browser;
  try {
    await server.listen();
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
use react::Element;
use react::html::p;
pub fn App() -> Element { p().children("Committed") }
`);
  await build({ root: dir, configFile: false, plugins: [rustJs(), react()], logLevel: "silent" });
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
use react::{Element, Memo, Provider, component, memo, use_context, use_state};
use react::html::button;
mod theme;
use theme::THEME;
thread_local! {
    static FAST_LABEL: Memo<LabelProps> = memo(Label);
}
pub struct LabelProps { pub text: &'static str }
pub fn Label(LabelProps { text }: LabelProps) -> Element {
    let theme = use_context(&THEME);
    let (n, set_n) = use_state(0);
    button().class_name(*theme).on_click(move |_| set_n.update(|n| n + 1)).children((text, " ", n))
}
pub fn App() -> Element {
    component(&THEME, Provider { value: "dark", children: component(&FAST_LABEL, LabelProps { text: "${label}" }) })
}
`;
  writeFileSync(join(dir, "src/App.rs"), app("Count"));
  const server = await createServer({ root: dir, configFile: false, plugins: [rustJs(), react()], logLevel: "silent", server: { port: 0 } });
  let browser;
  try {
    await server.listen();
    const theme = readFileSync(join(dir, "src/theme.js"), "utf8");
    expect(theme).toContain('export const THEME = createContext("light");');
    expect(readFileSync(join(dir, "src/App.jsx"), "utf8")).toContain('<theme$1.THEME value="dark">');
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
