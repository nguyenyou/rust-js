import { expect, test } from "bun:test";
import { mkdirSync, writeFileSync, readFileSync, unlinkSync, chmodSync, existsSync } from "node:fs";
import { join } from "node:path";
import { createServer, build } from "vite";
import react from "@vitejs/plugin-react";
import { chromium } from "@playwright/test";
import rustJs from "../vite-plugin/index.js";
import { buildCompiler, compiler, fixture } from "./support";

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
use react::html::button;
mod text;
pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    button().on_click(move |_| set_count.update(|n| n + 1)).children((text::label(), count))
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
