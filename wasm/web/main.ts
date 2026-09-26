// The playground: a Rust crate in (a few files), one JS file per module out
// (ADR 0019). rust-js.wasm runs on an in-memory WASI filesystem:
//
//   /in/lib.rs, /in/stats.rs, ...   the crate, from the Rust editor
//   /out/lib.js, /out/stats.js, ... what rust-js writes (plus .js.map files)
//   /sysroot/...                    the std metadata rustc type-checks against
//   /web/libweb.rmeta               the web crate's metadata (ADR 0024)
//
// Each compile gets a fresh instance of the (compiled once) module: rustc
// keeps global state, and a failed compile ends in a trap.

import type { File, Inode } from "@bjorn3/browser_wasi_shim";
import { javascript } from "@codemirror/lang-javascript";
import { rust } from "@codemirror/lang-rust";
import { Compartment, EditorState, type Extension, Prec } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { keymap } from "@codemirror/view";
import { basicSetup, EditorView } from "codemirror";

// The part of the playground written in Rust: rust/lib.rs, which build.ts
// and serve.ts compile to rust/lib.js with rust-js itself (compile-rust.ts).
import { compile, link as linkModules, load, mb, ms, render_tree, resolve as resolveSpecifier, stat } from "./rust/lib.js";

type Example = { name: string; title: string; root: string; files: string[] };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const exampleSelect = $<HTMLSelectElement>("example");
const button = $<HTMLButtonElement>("compile");
const testButton = $<HTMLButtonElement>("test");
const status = $<HTMLSpanElement>("status");

function setStatus(text: string, kind: "" | "good" | "bad" = "") {
  status.textContent = text;
  status.className = kind;
}

// ── Editors ─────────────────────────────────────────────────────────────
// Rust in, JavaScript out. Both follow the system's light or dark setting.

const darkMode = window.matchMedia("(prefers-color-scheme: dark)");
const themeFor = (dark: boolean): Extension => (dark ? oneDark : []);
const sourceTheme = new Compartment();
const outputTheme = new Compartment();
const outputLanguage = new Compartment();

const sourceExtensions: Extension[] = [
  basicSetup,
  rust(),
  sourceTheme.of(themeFor(darkMode.matches)),
  // basicSetup binds Mod-Enter to "insert blank line": outrank it.
  Prec.highest(keymap.of([{ key: "Mod-Enter", run: () => (void onCompile(), true) }])),
  EditorView.contentAttributes.of({ "aria-label": "Rust source" }),
];
const source = new EditorView({ parent: $("source"), extensions: sourceExtensions });
// Read-only, but still selectable and copyable. Highlighted as JS for a
// generated file, plain text when it shows rustc's diagnostics.
const output = new EditorView({
  parent: $("output"),
  extensions: [
    basicSetup,
    outputLanguage.of(javascript()),
    outputTheme.of(themeFor(darkMode.matches)),
    EditorState.readOnly.of(true),
    EditorView.contentAttributes.of({ "aria-label": "Generated JavaScript" }),
  ],
});
darkMode.addEventListener("change", (e) => {
  source.dispatch({ effects: sourceTheme.reconfigure(themeFor(e.matches)) });
  output.dispatch({ effects: outputTheme.reconfigure(themeFor(e.matches)) });
});

// ── File explorer ───────────────────────────────────────────────────────
// `render_tree`, in rust/lib.rs: a button per file, folders as labels.

const renderTree = render_tree as (
  list: HTMLElement,
  paths: string[],
  options: {
    selected: string;
    first?: string;
    on_open: (path: string) => void;
    decorate?: (li: HTMLElement, path: string) => void;
  },
) => void;

// ── The crate being edited ──────────────────────────────────────────────
// Each file keeps its own editor state, so undo history survives switching.

let root = "lib.rs";
const files = new Map<string, EditorState>();
let current = "";

const newState = (text: string) => EditorState.create({ doc: text, extensions: sourceExtensions });

function openFile(path: string) {
  if (current && files.has(current)) files.set(current, source.state);
  current = path;
  source.setState(files.get(path)!);
  // A stored state has the theme from when it was created: bring it up to date.
  source.dispatch({ effects: sourceTheme.reconfigure(themeFor(darkMode.matches)) });
  renderSourceFiles();
}

function renderSourceFiles() {
  renderTree($("source-files"), [...files.keys()], {
    selected: current,
    first: root,
    on_open: openFile,
    decorate: (li, path) => {
      if (path === root) {
        const note = document.createElement("span");
        note.className = "note";
        note.textContent = "root ";
        li.append(note);
        return;
      }
      const remove = document.createElement("button");
      remove.className = "delete";
      remove.textContent = "×";
      remove.setAttribute("aria-label", `Delete ${path}`);
      remove.addEventListener("click", () => {
        if (!confirm(`Delete ${path}?`)) return;
        files.delete(path);
        if (current === path) {
          current = "";
          openFile(root);
        } else {
          renderSourceFiles();
        }
      });
      li.append(remove);
    },
  });
}

$("new-file").addEventListener("click", () => {
  const path = prompt("New file, e.g. math.rs or geometry/shape.rs:")?.trim();
  if (!path) return;
  if (!/^([a-z_][a-z0-9_]*\/)*[a-z_][a-z0-9_]*\.rs$/.test(path)) {
    setStatus(`"${path}" isn't a Rust module file name, like math.rs or geometry/shape.rs.`, "bad");
    return;
  }
  if (files.has(path)) {
    setStatus(`${path} already exists.`, "bad");
    return;
  }
  files.set(path, newState(""));
  openFile(path);
  const module = path.slice(path.lastIndexOf("/") + 1, -".rs".length);
  setStatus(`Created ${path}. Declare it with \`mod ${module};\` in its parent, or rustc won't include it.`);
});

/** The crate's files as text, including unsaved edits in the open file. */
function crateSources(): Map<string, string> {
  files.set(current, source.state);
  return new Map([...files].map(([path, state]) => [path, state.doc.toString()]));
}

// ── Generated JS ────────────────────────────────────────────────────────

let outputs = new Map<string, string>();
let shownOutput = "";

const rootJs = () => root.replace(/\.rs$/, ".js");

function openOutput(path: string) {
  shownOutput = path;
  output.dispatch({
    changes: { from: 0, to: output.state.doc.length, insert: outputs.get(path)! },
    effects: outputLanguage.reconfigure(javascript()),
  });
  renderOutputFiles();
}

function renderOutputFiles() {
  const list = $<HTMLUListElement>("output-files");
  if (outputs.size === 0) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = "(none)";
    list.replaceChildren(empty);
    return;
  }
  renderTree(list, [...outputs.keys()], { selected: shownOutput, first: rootJs(), on_open: openOutput });
}

function showDiagnostics(text: string) {
  outputs = new Map();
  shownOutput = "";
  output.dispatch({
    changes: { from: 0, to: output.state.doc.length, insert: text },
    effects: outputLanguage.reconfigure([]),
  });
  renderOutputFiles();
}

// ── Running rust-js ─────────────────────────────────────────────────────
// `compile`, in rust/lib.rs: rust-js.wasm on the crate, under the WASI shim.

/** What `compile` gives back: `exit` is the exit code, or how it trapped. */
type Result = {
  exit: string;
  ok: boolean;
  files: Map<string, string>;
  stderr: string;
  instantiate: number;
  run: number;
  memory: number;
};

// ── Running the program ─────────────────────────────────────────────────
// If the root module exports `main`, run it in a frame with a
// `<div id="app">` to render into. The modules are linked into one plain
// `<script>` (see `link`), which every browser runs the same way, and the
// page reports back whether `main` ran, so the status line always says.
//
// The frame isn't sandboxed. Chrome runs a sandboxed frame in a process of
// its own, and some setups then don't draw it until something else changes
// the layout: the program ran, but the frame stayed blank. The program is
// the one in the editor, so it may share this page's origin.

const resultSection = $<HTMLElement>("result-section");
let resultFrame = $<HTMLIFrameElement>("result");

// `resolve` and `link`, in rust/lib.rs, join the modules into one classic
// script: each module a function filling in its exports object.
const resolve = resolveSpecifier as (from: string, specifier: string) => string;
const link = linkModules as (files: Map<string, string>, start: string) => string;

let programRuns = 0;
let reported = false;

// A small `bun test` look-alike for the Result frame: `test` and `test.skip`
// collect the tests, which then run one after another. What they leave in
// the page is replaced by the report.
const TEST_RUNNER = `
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
    const count = (outcome) => results.filter((r) => r.outcome === outcome).length;`;

/** Run the root module's `main()`, or with `test`, the crate's tests. */
function runProgram(files: Map<string, string>, rootFile: string, test = false) {
  const main = files.get(rootFile);
  const tests = rootFile.replace(/\.js$/, ".test.js");
  programRuns++;
  const runnable = test ? files.has(tests) : main !== undefined && /^export function main\(\)/m.test(main);
  // Imports from JS modules (ADR 0028) name packages or files the page
  // doesn't have. A bundler would bring them in; the playground has none.
  const external = new Set<string>();
  for (const [path, code] of files) {
    for (const [, specifier] of code.matchAll(/^import .* from "([^"]+)";$/gm)) {
      if (!files.has(resolve(path, specifier))) external.add(specifier);
    }
  }
  if (!runnable || external.size > 0) {
    resultSection.hidden = true;
    resultFrame.srcdoc = "";
    if (runnable) {
      const names = [...external].map((s) => `"${s}"`).join(", ");
      setStatus(`${status.textContent} Not run: it imports ${names}, which the playground can't load. Bundle it with bun build.`, "bad");
    }
    return;
  }
  const run = programRuns;
  const report = (message: string) => `parent.postMessage({ run: ${run}, ${message} }, "*")`;
  reported = false;
  // If the page never reports, say so: something stopped its script.
  setTimeout(() => {
    if (run === programRuns && !reported) {
      setStatus("The Result frame didn't run. Is something blocking its script? See the console.", "bad");
    }
  }, 3000);
  resultSection.hidden = false;
  // A new frame each run: the program starts from a clean page, and a frame
  // made while its section is showing gets drawn right away.
  const frame = resultFrame.cloneNode() as HTMLIFrameElement;
  resultFrame.replaceWith(frame);
  resultFrame = frame;
  resultFrame.srcdoc = `<!doctype html>
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
<div id="app"></div>
<script>
  // Errors later on, in an event handler say.
  addEventListener("error", (e) => ${report("error: String(e.message)")});
  // And in async code, which rejects its promise instead (ADR 0029).
  addEventListener("unhandledrejection", (e) => ${report("error: String(e.reason)")});
  // What a test file calls, as bun test provides it (ADR 0026).
  const registered = [];
  globalThis.test = (name, f) => registered.push({ name, f });
  test.skip = (name) => registered.push({ name });
</script>
<script>
  try {
${test ? link(files, TEST_RUNNER) : link(files, `modules[${JSON.stringify(rootFile)}].main();`)}
    ${test ? report(`tested: { passed: count("pass"), failed: count("fail"), ignored: count("skip") }`) : report("ran: true")};
  } catch (e) {
    ${report("error: String(e)")};
  }
</script>`;
}

addEventListener("message", (e) => {
  if (e.source !== resultFrame.contentWindow || e.data?.run !== programRuns) return;
  reported = true;
  if (e.data.error) setStatus(`Runtime error: ${e.data.error}`, "bad");
  else if (e.data.ran) setStatus(`${status.textContent} Ran main().`, "good");
  else if (e.data.tested) {
    const { passed, failed, ignored } = e.data.tested;
    const total = passed + failed;
    const summary = total === 0 ? "No tests." : `Tests: ${passed} passed, ${failed} failed${ignored ? `, ${ignored} ignored` : ""}.`;
    setStatus(summary, failed ? "bad" : "good");
  }
});

// ── Loading ─────────────────────────────────────────────────────────────
// Downloading the compiler, the sysroot, the web crate and the examples is
// `load`, in rust/lib.rs.

async function loadExample(example: Example) {
  const texts = await Promise.all(
    example.files.map(async (f) => [f, await (await fetch(`./examples/${example.name}/${f}`)).text()] as const),
  );
  files.clear();
  for (const [path, text] of texts) files.set(path, newState(text));
  root = example.root;
  current = "";
  openFile(root);
  outputs = new Map();
  shownOutput = "";
  output.dispatch({ changes: { from: 0, to: output.state.doc.length, insert: "" } });
  renderOutputFiles();
  runProgram(new Map(), rootJs());
}

const { module, sysroot, web_crate: webCrate, examples } = (await load()) as {
  module: WebAssembly.Module;
  sysroot: Map<string, Inode>;
  web_crate: File;
  examples: Example[];
};
for (const example of examples) exampleSelect.add(new Option(example.title, example.name));
exampleSelect.addEventListener("change", async () => {
  await loadExample(examples.find((e) => e.name === exampleSelect.value)!);
  setStatus("Ready.");
});
await loadExample(examples[0]);
button.disabled = false;
testButton.disabled = false;
setStatus("Ready.");

let runs = 0;
let compiling = false;
async function onCompile(test = false) {
  if (compiling || button.disabled) return;
  compiling = true;
  button.disabled = testButton.disabled = true;
  setStatus(test ? "Compiling the tests…" : "Compiling…");
  const r = (await compile(module, sysroot, webCrate, crateSources(), root, test)) as Result;
  runs++;
  const ok = r.ok;
  if (ok) {
    outputs = r.files;
    // Keep showing the same file if it's still there; otherwise the root's.
    openOutput(outputs.has(shownOutput) ? shownOutput : rootJs());
    setStatus(`Compiled: ${outputs.size} JS file${outputs.size === 1 ? "" : "s"}.`, "good");
    runProgram(outputs, rootJs(), test);
  } else {
    showDiagnostics(r.stderr);
    runProgram(new Map(), rootJs());
    setStatus(`Failed: exit ${r.exit}.`, "bad");
  }
  stat(`compile #${runs}`, `instantiate ${ms(r.instantiate)}, run ${ms(r.run)}, memory ${mb(r.memory)}, ${ok ? "ok" : "error"}`);
  compiling = false;
  button.disabled = testButton.disabled = false;
  // For automated checks.
  (window as unknown as { lastResult: Result }).lastResult = r;
}

button.addEventListener("click", () => onCompile());
testButton.addEventListener("click", () => onCompile(true));
