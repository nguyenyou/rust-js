// Run rust-js.wasm in the browser, on an in-memory WASI filesystem:
//
//   /in/main.rs      the source from the textarea
//   /out/main.js     what rust-js writes (plus main.js.map)
//   /sysroot/...     the std metadata rustc type-checks against
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
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { basicSetup, EditorView } from "codemirror";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

// Two CodeMirror editors: Rust in, JavaScript out. Both follow the system's
// light or dark setting, like the rest of the page.
const darkMode = window.matchMedia("(prefers-color-scheme: dark)");
const themeFor = (dark: boolean): Extension => (dark ? oneDark : []);
const sourceTheme = new Compartment();
const outputTheme = new Compartment();
const outputLanguage = new Compartment();

const source = new EditorView({
  parent: $("source"),
  extensions: [
    basicSetup,
    rust(),
    sourceTheme.of(themeFor(darkMode.matches)),
    EditorView.contentAttributes.of({ "aria-label": "Rust source" }),
  ],
});
// Read-only, but still selectable and copyable. Highlighted as JS after a
// successful compile, plain text when it shows rustc's diagnostics.
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

function setText(view: EditorView, text: string, ...effects: ReturnType<Compartment["reconfigure"]>[]) {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text }, effects });
}
const status = $<HTMLSpanElement>("status");
const button = $<HTMLButtonElement>("compile");
const stats = $<HTMLTableElement>("stats");

const ms = (t: number) => `${t.toFixed(0)} ms`;
const mb = (n: number) => `${(n / 1048576).toFixed(1)} MB`;

function stat(label: string, value: string) {
  const row = stats.insertRow();
  row.insertCell().textContent = label;
  row.insertCell().textContent = value;
}

function dir(entries: Record<string, Inode>): Directory {
  return new Directory(new Map(Object.entries(entries)));
}

async function load() {
  const start = performance.now();
  const [module, sysroot, example] = await Promise.all([
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
      .then((files) => {
        const size = files.reduce((n, [, f]) => n + (f as File).data.byteLength, 0);
        stat("download sysroot", `${ms(performance.now() - start)} (${files.length} files, ${mb(size)})`);
        return new Map(files);
      }),
    fetch("./fib.rs").then((r) => r.text()),
  ]);
  stat("ready after", ms(performance.now() - start));
  return { module, sysroot, example };
}

type Result = { exit: number | string; js?: string; stderr: string; instantiate: number; run: number; memory: number };

async function compile(module: WebAssembly.Module, sysroot: Map<string, Inode>, code: string): Promise<Result> {
  const stderr: string[] = [];
  const outDir = new PreopenDirectory("/out", new Map());
  const fds = [
    new OpenFile(new File([])), // stdin
    ConsoleStdout.lineBuffered((line) => stderr.push(line)), // stdout
    ConsoleStdout.lineBuffered((line) => stderr.push(line)), // stderr
    new PreopenDirectory("/in", new Map([["main.rs", new File(new TextEncoder().encode(code))]])),
    outDir,
    new PreopenDirectory(
      "/sysroot",
      new Map([["lib", dir({ rustlib: dir({ "wasm32-unknown-unknown": dir({ lib: new Directory(sysroot) }) }) })]]),
    ),
  ];
  const args = ["rust-js", "/in/main.rs", "-o", "/out/main.js", "--", "--target", "wasm32-unknown-unknown", "--sysroot", "/sysroot"];
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

  const file = outDir.dir.contents.get("main.js") as File | undefined;
  const memory = (instance.exports.memory as WebAssembly.Memory).buffer.byteLength;
  return {
    exit,
    js: file && new TextDecoder().decode(file.data),
    stderr: stderr.join("\n"),
    instantiate: t1 - t0,
    run: t2 - t1,
    memory,
  };
}

const { module, sysroot, example } = await load();
setText(source, example);
button.disabled = false;
status.textContent = "Ready.";

let runs = 0;
async function onCompile() {
  button.disabled = true;
  status.textContent = "Compiling…";
  status.className = "";
  const r = await compile(module, sysroot, source.state.doc.toString());
  runs++;
  const ok = r.js !== undefined;
  setText(output, ok ? r.js! : r.stderr, outputLanguage.reconfigure(ok ? javascript() : []));
  status.textContent = ok ? `Compiled (exit ${r.exit}).` : `Failed: exit ${r.exit}.`;
  status.className = ok ? "good" : "bad";
  stat(`compile #${runs}`, `instantiate ${ms(r.instantiate)}, run ${ms(r.run)}, memory ${mb(r.memory)}, ${ok ? "ok" : "error"}`);
  button.disabled = false;
  // For automated checks.
  (window as unknown as { lastResult: Result }).lastResult = r;
}

button.addEventListener("click", onCompile);
