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

import {
  ConsoleStdout,
  Directory,
  File,
  type Inode,
  OpenFile,
  PreopenDirectory,
  WASI,
} from "@bjorn3/browser_wasi_shim";
import { javascript } from "@codemirror/lang-javascript";
import { rust } from "@codemirror/lang-rust";
import { Compartment, EditorState, type Extension, Prec } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { keymap } from "@codemirror/view";
import { basicSetup, EditorView } from "codemirror";

type Example = { name: string; title: string; root: string; files: string[] };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const exampleSelect = $<HTMLSelectElement>("example");
const button = $<HTMLButtonElement>("compile");
const testButton = $<HTMLButtonElement>("test");
const status = $<HTMLSpanElement>("status");
const stats = $<HTMLTableElement>("stats");

const ms = (t: number) => `${t.toFixed(0)} ms`;
const mb = (n: number) => `${(n / 1048576).toFixed(1)} MB`;

function stat(label: string, value: string) {
  const row = stats.insertRow();
  row.insertCell().textContent = label;
  row.insertCell().textContent = value;
}

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

type Tree = Map<string, Tree | string>; // a folder's entries: subfolders, or a file's full path

function buildTree(paths: string[]): Tree {
  const tree: Tree = new Map();
  for (const path of paths) {
    const parts = path.split("/");
    let folder = tree;
    for (const part of parts.slice(0, -1)) {
      if (!(folder.get(part) instanceof Map)) folder.set(part, new Map());
      folder = folder.get(part) as Tree;
    }
    folder.set(parts.at(-1)!, path);
  }
  return tree;
}

/** Render a file tree into `list`: a button per file, folders as labels. */
function renderTree(
  list: HTMLUListElement,
  paths: string[],
  options: {
    selected: string;
    first?: string;
    onOpen: (path: string) => void;
    decorate?: (li: HTMLLIElement, path: string) => void;
  },
) {
  const render = (tree: Tree, into: HTMLUListElement, depth: number) => {
    // The crate root first; then by name, a module's file just before its
    // folder: `geometry.rs`, then `geometry/` ("." sorts before "/").
    const key = ([name, entry]: [string, Tree | string]) => (entry instanceof Map ? `${name}/` : name);
    const entries = [...tree].sort((a, b) =>
      a[1] === options.first ? -1 : b[1] === options.first ? 1 : key(a) < key(b) ? -1 : 1,
    );
    for (const [name, entry] of entries) {
      const li = document.createElement("li");
      const indent = `${8 + depth * 12}px`;
      if (entry instanceof Map) {
        const label = document.createElement("span");
        label.className = "folder";
        label.style.paddingLeft = indent;
        label.textContent = `${name}/`;
        const nested = document.createElement("ul");
        render(entry, nested, depth + 1);
        const wrapper = document.createElement("div");
        wrapper.style.width = "100%";
        wrapper.append(label, nested);
        li.append(wrapper);
      } else {
        const file = document.createElement("button");
        file.className = "file";
        file.style.paddingLeft = indent;
        file.textContent = name;
        file.setAttribute("aria-current", String(entry === options.selected));
        file.addEventListener("click", () => options.onOpen(entry));
        li.append(file);
        options.decorate?.(li, entry);
      }
      into.append(li);
    }
  };
  list.replaceChildren();
  render(buildTree(paths), list, 0);
}

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
    onOpen: openFile,
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
  renderTree(list, [...outputs.keys()], { selected: shownOutput, first: rootJs(), onOpen: openOutput });
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

function dir(entries: Record<string, Inode>): Directory {
  return new Directory(new Map(Object.entries(entries)));
}

/** A WASI directory tree from `path → text`, e.g. `geometry/area.rs`. */
function directoryOf(sources: Map<string, string>): Map<string, Inode> {
  const top = new Map<string, Inode>();
  for (const [path, text] of sources) {
    const parts = path.split("/");
    let folder = top;
    for (const part of parts.slice(0, -1)) {
      if (!folder.has(part)) folder.set(part, new Directory(new Map()));
      folder = (folder.get(part) as Directory).contents;
    }
    folder.set(parts.at(-1)!, new File(new TextEncoder().encode(text)));
  }
  return top;
}

/** Every `.js` file under a WASI directory, as `path → text`. */
function jsFilesIn(folder: Directory, prefix = "", found = new Map<string, string>()): Map<string, string> {
  for (const [name, entry] of folder.contents) {
    if (entry instanceof Directory) jsFilesIn(entry, `${prefix}${name}/`, found);
    else if (entry instanceof File && name.endsWith(".js")) found.set(prefix + name, new TextDecoder().decode(entry.data));
  }
  return found;
}

type Result = {
  exit: number | string;
  files: Map<string, string>;
  stderr: string;
  instantiate: number;
  run: number;
  memory: number;
};

async function compile(
  module: WebAssembly.Module,
  sysroot: Map<string, Inode>,
  webCrate: File,
  sources: Map<string, string>,
  rootFile: string,
  test: boolean,
): Promise<Result> {
  const stderr: string[] = [];
  const outDir = new PreopenDirectory("/out", new Map());
  const fds = [
    new OpenFile(new File([])), // stdin
    ConsoleStdout.lineBuffered((line) => stderr.push(line)), // stdout
    ConsoleStdout.lineBuffered((line) => stderr.push(line)), // stderr
    new PreopenDirectory("/in", directoryOf(sources)),
    outDir,
    new PreopenDirectory(
      "/sysroot",
      new Map([["lib", dir({ rustlib: dir({ "wasm32-unknown-unknown": dir({ lib: new Directory(sysroot) }) }) })]]),
    ),
    new PreopenDirectory("/web", new Map([["libweb.rmeta", webCrate]])),
  ];
  const outFile = `/out/${rootFile.replace(/\.rs$/, ".js")}`;
  // `--test`: the `#[test]` functions too, and `<root>.test.js` to run them (ADR 0026).
  // This is a real browser, so tests marked `#[cfg(browser)]` run too (ADR 0027).
  const mode = test ? ["--test"] : [];
  const cfg = test ? ["--cfg=browser"] : [];
  const args = ["rust-js", ...mode, `/in/${rootFile}`, "-o", outFile, "--", "--target", "wasm32-unknown-unknown", "--sysroot", "/sysroot", ...cfg];
  // Every program may use the web crate; rustc only reads it if one does.
  args.push("--extern", "web=/web/libweb.rmeta");
  // RUSTC_ICE=0: don't name a crash-report file after the process id (WASI has none).
  const wasi = new WASI(args, ["RUSTC_ICE=0"], fds);

  const t0 = performance.now();
  const instance = (await WebAssembly.instantiate(module, {
    wasi_snapshot_preview1: wasi.wasiImport,
  })) as WebAssembly.Instance;
  const t1 = performance.now();
  let exit: number | string;
  try {
    exit = wasi.start(instance as { exports: { memory: WebAssembly.Memory; _start: () => unknown } });
  } catch (e) {
    // Errors end in a trap: panics can't unwind on wasm32-wasip1.
    exit = `trap (${e instanceof Error ? e.message : String(e)})`;
  }
  const t2 = performance.now();

  return {
    exit,
    files: exit === 0 ? jsFilesIn(outDir.dir) : new Map(),
    stderr: stderr.join("\n"),
    instantiate: t1 - t0,
    run: t2 - t1,
    memory: (instance.exports.memory as WebAssembly.Memory).buffer.byteLength,
  };
}

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

/** `from`'s directory joined with a relative specifier like `../lib.js`. */
function resolve(from: string, specifier: string): string {
  const parts = from.split("/").slice(0, -1);
  for (const part of specifier.split("/")) {
    if (part === "..") parts.pop();
    else if (part !== ".") parts.push(part);
  }
  return parts.join("/");
}

/**
 * rust-js's modules (ADR 0019) as one classic script. Each module becomes a
 * function that fills in its exports object, and `import * as util from
 * "./util.js"` becomes that module's exports object. The objects all exist
 * before any module runs, so cycles work: functions are only called later.
 */
function link(files: Map<string, string>, start: string): string {
  const key = (path: string) => JSON.stringify(path);
  const parts = ["const modules = {};", ...[...files.keys()].map((path) => `modules[${key(path)}] = {};`)];
  // A test file reads the tests' functions as it registers them, so it goes
  // after the modules that define them.
  const ordered = [...files].sort(([a], [b]) => Number(a.endsWith(".test.js")) - Number(b.endsWith(".test.js")));
  for (const [path, code] of ordered) {
    const exported: string[] = [];
    const body = code
      .replace(/^import \* as (\S+) from "([^"]+)";$/gm, (_, alias, specifier) => {
        return `const ${alias} = modules[${key(resolve(path, specifier))}];`;
      })
      .replace(/^export function (\w+)/gm, (_, name) => {
        exported.push(name);
        return `function ${name}`;
      })
      .replace(/^\/\/# sourceMappingURL=.*$/m, "");
    parts.push(`(function (exports) {\n${body}\nObject.assign(exports, { ${exported.join(", ")} });\n})(modules[${key(path)}]);`);
  }
  parts.push(start);
  // A `</script>` in a string would end the script early; `<\/script>` is the same string.
  return parts.join("\n").replaceAll("</script", "<\\/script");
}

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
  if (!runnable) {
    resultSection.hidden = true;
    resultFrame.srcdoc = "";
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

async function load() {
  const start = performance.now();
  const [module, sysroot, webCrate, examples] = await Promise.all([
    WebAssembly.compileStreaming(fetch("./rust-js.wasm")).then((m) => {
      stat("download + compile rust-js.wasm", ms(performance.now() - start));
      return m;
    }),
    fetch("./sysroot.json")
      .then((r) => r.json() as Promise<string[]>)
      .then((names) =>
        Promise.all(
          names.map(async (name) => {
            const bytes = new Uint8Array(await (await fetch(`./sysroot/${name}`)).arrayBuffer());
            return [name, new File(bytes, { readonly: true })] as [string, Inode];
          }),
        ),
      )
      .then((entries) => {
        const size = entries.reduce((n, [, f]) => n + (f as File).data.byteLength, 0);
        stat("download sysroot", `${ms(performance.now() - start)} (${entries.length} files, ${mb(size)})`);
        return new Map(entries);
      }),
    fetch("./web/libweb.rmeta")
      .then((r) => r.arrayBuffer())
      .then((bytes) => {
        stat("download web crate", `${ms(performance.now() - start)} (${mb(bytes.byteLength)})`);
        return new File(new Uint8Array(bytes), { readonly: true });
      }),
    fetch("./examples.json").then((r) => r.json() as Promise<Example[]>),
  ]);
  stat("ready after", ms(performance.now() - start));
  return { module, sysroot, webCrate, examples };
}

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

const { module, sysroot, webCrate, examples } = await load();
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
  const r = await compile(module, sysroot, webCrate, crateSources(), root, test);
  runs++;
  const ok = r.exit === 0;
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
