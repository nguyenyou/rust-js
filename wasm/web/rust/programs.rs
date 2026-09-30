// Running the compiled program. If the root module exports `main`, it runs
// in a frame with a `<div id="app">` to render into; with Test, the crate's
// tests run there instead. An import map links the generated ES modules
// (see `link`), preserving live bindings and cycles, and the page reports
// back what happened, so the status line always says.
//
// The frame has an opaque origin and allows scripts only. Programs can report
// results through postMessage, but cannot access the editor's DOM or storage.

use std::collections::HashMap;

use js::{JsObject, RegExp, encode_uri_component, json, reg_exp};

// A replacer and `matchAll`, typed for the patterns here: a JS replacer is
// given the match, then each group, so its type is its pattern's (js::RegExp).
unsafe extern "Rust" {
    /// `text.replace(pattern, (match, a, b) => ..)`: a closure for each match.
    #[link_name = "replace"]
    safe fn replace_matches(
        this: &str,
        pattern: &RegExp,
        with: Box<dyn Fn(String, String, String) -> String>,
    ) -> String;
    #[link_name = "matchAll"]
    safe fn match_all(this: &str, pattern: &RegExp) -> &'static JsObject;
    /// Each match of a pattern with one group, as `(match, group)`.
    #[link_name = "Array.from"]
    safe fn matches_of(matches: &JsObject) -> Vec<(String, String)>;
}

/// Sucrase's options: JSX as `react/jsx-runtime`'s calls, as a bundler writes it.
pub struct TransformOptions {
    pub transforms: Vec<&'static str>,
    #[rust_js::name = "jsxRuntime"]
    pub jsx_runtime: &'static str,
    pub production: bool,
}

pub struct Transformed {
    pub code: String,
}

/// [Sucrase](https://github.com/alangpierce/sucrase): the JSX rust-js writes,
/// as the JS the frame can run, which has no bundler.
#[rust_js::link_name = "sucrase#transform"]
#[allow(unused_variables)]
fn transform(code: &str, options: TransformOptions) -> Transformed {
    unreachable!()
}

/// A program to run in the Result frame: its page, and which run it is, so a
/// report from an older one is ignored.
pub struct Program {
    pub run: u32,
    pub page: String,
}

/// What the frame's page posts back. Fields it doesn't send are `undefined`.
pub struct Report {
    pub run: Option<u32>,
    pub error: Option<String>,
    pub ran: Option<bool>,
    pub tested: Option<Tested>,
    /// How tall the page is, as it grows: the frame is as tall.
    pub height: Option<f64>,
}

pub struct Tested {
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
}

/// How a run went.
pub enum Outcome {
    Ran,
    Tested(Tested),
    Failed(String),
    /// It never reported: something stopped its script.
    Silent,
}

/// What `prepare` found to run.
pub enum Prepared {
    /// No `main()`, or with Test, no tests.
    Nothing,
    /// It imports JS the playground can't load: these specifiers.
    Blocked(Vec<String>),
    Page(String),
}

/// `from`'s directory joined with a relative specifier like `../lib.js`. A
/// package's, `@rust-js/runtime`, is itself.
pub fn resolve(from: &str, specifier: &str) -> String {
    if !specifier.starts_with('.') {
        return specifier.to_string();
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in specifier.split('/') {
        if part == ".." {
            parts.pop();
        } else if part != "." {
            parts.push(part);
        }
    }
    parts.join("/")
}

/// Link generated ES modules through an import map. Virtual specifiers avoid
/// embedding URLs recursively, so cycles work. The browser owns module
/// evaluation, named imports and live bindings; no identifier rewriting.
pub fn link(files: &[(String, String)]) -> String {
    let imports = reg_exp::new(r#"^import ([^;]+?) from "([^"]+)";"#, "gm");
    let source_map = reg_exp::new(r"^//# sourceMappingURL=.*$", "m");
    let mut entries = Vec::new();
    for (path, code) in files.iter().cloned() {
        let from = path.clone();
        let body = replace_matches(
            &code,
            imports,
            Box::new(move |_, names, specifier| {
                let target = json::stringify(&format!("rust-js:{}", resolve(&from, &specifier)));
                format!("import {names} from {target};")
            }),
        );
        let body = reg_exp::replace(&body, source_map, "");
        let specifier = json::stringify(&format!("rust-js:{path}"));
        // Identical module bodies must still have separate state.
        let url = json::stringify(&format!(
            "data:text/javascript,{}#{}",
            encode_uri_component(&body),
            encode_uri_component(&path)
        ));
        entries.push(format!("{specifier}: {url}"));
    }
    let entries = entries.join(",");
    format!(r#"<script type="importmap">{{"imports":{{{entries}}}}}</script>"#)
}

/// A small `bun test` look-alike for the Result frame: `test` and `test.skip`
/// collect the tests, which then run one after another. What they leave in
/// the page is replaced by the report.
const TEST_RUNNER: &str = r#"
    const results = registered.map(({ name, f }) => {
      if (!f) return { name, outcome: "skip" };
      try {
        f();
        return { name, outcome: "pass" };
      } catch (e) {
        return { name, outcome: "fail", message: e instanceof Error ? e.message : String(e) };
      }
    });
    document.body.replaceChildren(...results.map(({ name, outcome, message }) => {
      const line = document.createElement("div");
      line.className = outcome;
      line.textContent = { pass: "✓ ", fail: "✗ ", skip: "– " }[outcome] + name + (outcome === "skip" ? " (ignored)" : "");
      if (message) {
        const why = document.createElement("pre");
        why.textContent = message;
        line.append(why);
      }
      return line;
    }));
    const count = (outcome) => results.filter((r) => r.outcome === outcome).length;"#;

/// The frame's style, before its scripts.
// A link to `#/active` is this page's, as a program's routes need: a
// sandboxed `srcdoc` page's links resolve against its parent's URL
// otherwise, and navigate the frame away.
const FRAME_HEAD: &str = r#"<!doctype html>
<meta charset="utf-8">
<base href="about:srcdoc">"#;

/// The frame's own look, for a program that brings none.
const FRAME_STYLE: &str = r#"<style>
  :root { color-scheme: light dark; font: 15px/1.5 system-ui, sans-serif; }
  body { margin: 12px; }
  button { font: inherit; min-width: 2.5em; padding: 2px 10px; }
  output { display: inline-block; min-width: 3em; text-align: center; font-variant-numeric: tabular-nums; }
  .pass { color: #2f6b3a; } .fail { color: #a3321f; } .skip { color: #6b6b66; }
  @media (prefers-color-scheme: dark) { .pass { color: #8fcf98; } .fail { color: #ef8a78; } }
  pre { margin: 2px 0 8px 1.5em; white-space: pre-wrap; font-size: 13px; }
</style>"#;

/// The page that runs the root module's `main()`, or with `test`, the
/// crate's tests, and reports as run number `run`.
pub fn prepare(
    files: &HashMap<String, String>,
    modules: &[(String, String)],
    styles: &[(String, String)],
    root_file: &str,
    test: bool,
    run: u32,
) -> Prepared {
    // The program's modules, its JSX as JS (Sucrase's), and what they import
    // of the page's: the runtime, and React's.
    let mut sources: Vec<(String, String)> = files
        .iter()
        .map(|(path, code)| (path.clone(), code.clone()))
        .map(|(path, code)| match path.ends_with(".jsx") {
            true => {
                let options = TransformOptions {
                    transforms: vec!["jsx"],
                    jsx_runtime: "automatic",
                    production: true,
                };
                let code = transform(&code, options).code;
                (path, code)
            }
            false => (path, code),
        })
        .collect();
    sources.extend(modules.iter().cloned());
    let tests = match root_file.strip_suffix(".jsx").or_else(|| root_file.strip_suffix(".js")) {
        Some(stem) => format!("{stem}.test.js"),
        None => root_file.to_string(),
    };
    // `main`, sync or async (ADR 0029).
    let has_main = reg_exp::new(r"^export (async )?function main\(\)", "m");
    let runnable = if test {
        sources.iter().any(|(path, _)| *path == tests)
    } else {
        sources
            .iter()
            .any(|(path, code)| path == root_file && reg_exp::test(has_main, code))
    };
    if !runnable {
        return Prepared::Nothing;
    }
    // A stylesheet a module imports, `import "todomvc-app-css/index.css";`
    // (ADR 0028), is the page's, as a bundler makes it: in a `<style>`, in
    // the order imported, and the program's look, not the frame's.
    let css_imports = reg_exp::new(r#"^import "([^"]+\.css)"(;)"#, "gm");
    let mut css: Vec<String> = Vec::new();
    for (_, code) in sources.iter_mut() {
        for (_, specifier) in matches_of(match_all(code, css_imports)) {
            if let Some((_, text)) = styles.iter().find(|(name, _)| *name == specifier)
                && !css.contains(text)
            {
                css.push(text.clone());
            }
        }
        let known: Vec<String> = styles.iter().map(|(name, _)| name.clone()).collect();
        *code = replace_matches(
            code,
            css_imports,
            Box::new(move |whole, specifier, _| {
                if known.contains(&specifier) {
                    String::new()
                } else {
                    whole
                }
            }),
        );
    }
    let look = match css.is_empty() {
        true => FRAME_STYLE.to_string(),
        false => format!("<style>\n{}</style>", css.concat()),
    };
    // Imports from JS modules (ADR 0028) name packages or files the page
    // doesn't have. A bundler would bring them in; the playground has none.
    let imports = reg_exp::new(r#"^import (?:[^;]+? from )?"([^"]+)";"#, "gm");
    let mut external: Vec<String> = Vec::new();
    for (path, code) in &sources {
        for (_, specifier) in matches_of(match_all(code, imports)) {
            let target = resolve(path, &specifier);
            if !sources.iter().any(|(p, _)| *p == target) && !external.contains(&specifier) {
                external.push(specifier);
            }
        }
    }
    if !external.is_empty() {
        return Prepared::Blocked(external);
    }
    let report = |message: &str| format!("parent.postMessage({{ run: {run}, {message} }}, \"*\")");
    let linked = link(&sources);
    let entry = json::stringify(&format!("rust-js:{}", if test { &tests } else { root_file }));
    let start = if test {
        format!("await import({entry});\n{TEST_RUNNER}")
    } else {
        format!("const root = await import({entry});\nawait root.main();")
    };
    let finished = if test {
        report(r#"tested: { passed: count("pass"), failed: count("fail"), ignored: count("skip") }"#)
    } else {
        report("ran: true")
    };
    Prepared::Page(format!(
        r#"{FRAME_HEAD}
{look}
<div id="app"></div>
{linked}
<script>
  // Errors later on, in an event handler say.
  addEventListener("error", (e) => {});
  // And in async code, which rejects its promise instead (ADR 0029).
  addEventListener("unhandledrejection", (e) => {});
  // What a test file calls, as bun test provides it (ADR 0026).
  const registered = [];
  globalThis.test = (name, f) => registered.push({{ name, f }});
  test.skip = (name) => registered.push({{ name }});
  // How tall the page is, each time it changes, for the frame to show it whole.
  new ResizeObserver(() => {}).observe(document.documentElement);
</script>
<script type="module">
  try {{
{start}
    {finished};
  }} catch (e) {{
    {};
  }}
</script>"#,
        report("error: String(e.message)"),
        report("error: String(e.reason)"),
        report("height: document.documentElement.scrollHeight"),
        report("error: String(e)"),
    ))
}

/// What a report says happened.
pub fn outcome(report: Report) -> Option<Outcome> {
    if let Some(error) = report.error {
        Some(Outcome::Failed(error))
    } else if report.ran == Some(true) {
        Some(Outcome::Ran)
    } else {
        match report.tested {
            Some(tested) => Some(Outcome::Tested(tested)),
            None => None,
        }
    }
}
