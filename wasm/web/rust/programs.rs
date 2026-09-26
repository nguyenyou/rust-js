// Running the compiled program. If the root module exports `main`, it runs
// in a frame with a `<div id="app">` to render into; with Test, the crate's
// tests run there instead. An import map links the generated ES modules
// (see `link`), preserving live bindings and cycles, and the page reports
// back what happened, so the status line always says.
//
// The frame isn't sandboxed. Chrome runs a sandboxed frame in a process of
// its own, and some setups then don't draw it until something else changes
// the layout: the program ran, but the frame stayed blank. The program is
// the one in the editor, so it may share this page's origin.

use web::{JsObject, RegExp, reg_exp};

use crate::compiler::{JsMap, text_entries};

// `replace` is one JS method, typed for each way it's called.
#[allow(clashing_extern_declarations)]
unsafe extern "Rust" {
    #[link_name = "JSON.stringify"]
    safe fn json_string(text: &str) -> String;
    #[link_name = "encodeURIComponent"]
    safe fn encode_uri_component(text: &str) -> String;
    /// `text.replace(pattern, (match, a, b) => ..)`: a closure for each match.
    #[link_name = "replace"]
    safe fn replace_matches(this: &str, pattern: &RegExp, with: Box<dyn Fn(String, String, String) -> String>) -> String;
    #[link_name = "replace"]
    safe fn replace_pattern(this: &str, pattern: &RegExp, with: &str) -> String;
    #[link_name = "matchAll"]
    safe fn match_all(this: &str, pattern: &RegExp) -> &'static JsObject;
    /// Each match of a pattern with one group, as `(match, group)`.
    #[link_name = "Array.from"]
    safe fn matches_of(matches: &JsObject) -> Vec<(String, String)>;
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
    /// The output needs a JSX transform and React runtime before it can run.
    Jsx,
    Page(String),
}

/// `from`'s directory joined with a relative specifier like `../lib.js`.
pub fn resolve(from: &str, specifier: &str) -> String {
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
pub fn link(files: &JsMap) -> String {
    let imports = reg_exp::new(r#"^import ([^;]+?) from "([^"]+)";$"#, "gm");
    let source_map = reg_exp::new(r"^//# sourceMappingURL=.*$", "m");
    let mut entries = Vec::new();
    for (path, code) in text_entries(files) {
        let from = path.clone();
        let body = replace_matches(
            &code,
            imports,
            Box::new(move |_, names, specifier| {
                let target = json_string(&format!("rust-js:{}", resolve(&from, &specifier)));
                format!("import {names} from {target};")
            }),
        );
        let body = replace_pattern(&body, source_map, "");
        let specifier = json_string(&format!("rust-js:{path}"));
        // Identical module bodies must still have separate state.
        let url = json_string(&format!("data:text/javascript,{}#{}", encode_uri_component(&body), encode_uri_component(&path)));
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
const FRAME_HEAD: &str = r#"<!doctype html>
<meta charset="utf-8">
<style>
  :root { color-scheme: light dark; font: 15px/1.5 system-ui, sans-serif; }
  body { margin: 12px; }
  button { font: inherit; min-width: 2.5em; padding: 2px 10px; }
  output { display: inline-block; min-width: 3em; text-align: center; font-variant-numeric: tabular-nums; }
  .pass { color: #2f6b3a; } .fail { color: #a3321f; } .skip { color: #6b6b66; }
  @media (prefers-color-scheme: dark) { .pass { color: #8fcf98; } .fail { color: #ef8a78; } }
  pre { margin: 2px 0 8px 1.5em; white-space: pre-wrap; font-size: 13px; }
</style>
<div id="app"></div>"#;

/// The page that runs the root module's `main()`, or with `test`, the
/// crate's tests, and reports as run number `run`.
pub fn prepare(files: &JsMap, root_file: &str, test: bool, run: u32) -> Prepared {
    let sources = text_entries(files);
    let tests = match root_file.strip_suffix(".jsx").or_else(|| root_file.strip_suffix(".js")) {
        Some(stem) => format!("{stem}.test.js"),
        None => root_file.to_string(),
    };
    // `main`, sync or async (ADR 0029).
    let has_main = reg_exp::new(r"^export (async )?function main\(\)", "m");
    let runnable = if test {
        sources.iter().any(|(path, _)| *path == tests)
    } else {
        sources.iter().any(|(path, code)| path == root_file && reg_exp::test(has_main, code))
    };
    if !runnable {
        return Prepared::Nothing;
    }
    if sources.iter().any(|(path, _)| path.ends_with(".jsx")) {
        return Prepared::Jsx;
    }
    // Imports from JS modules (ADR 0028) name packages or files the page
    // doesn't have. A bundler would bring them in; the playground has none.
    let imports = reg_exp::new(r#"^import (?:[^;]+? from )?"([^"]+)";$"#, "gm");
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
    let linked = link(files);
    let entry = json_string(&format!("rust-js:{}", if test { &tests } else { root_file }));
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
