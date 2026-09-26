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
import { compile, listen_for_reports, load, mb, ms, render_tree, run_program, set_status, stat } from "./rust/lib.js";

type Example = { name: string; title: string; root: string; files: string[] };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const exampleSelect = $<HTMLSelectElement>("example");
const button = $<HTMLButtonElement>("compile");
const testButton = $<HTMLButtonElement>("test");
// `set_status`, in rust/lib.rs: the line beside the buttons.
const setStatus = (text: string, kind: "" | "good" | "bad" = "") => set_status(text, kind);

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
// `run_program`, in rust/lib.rs: `main()` or the tests, in the Result frame,
// which reports back to `listen_for_reports`.

const runProgram = (files: Map<string, string>, rootFile: string, test = false) => run_program(files, rootFile, test);
listen_for_reports();

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
