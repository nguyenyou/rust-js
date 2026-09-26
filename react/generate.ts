// Generate, from each React release and from W3C's specs (ADR 0043):
//
//   react/versions.json  each React minor release, and which one first has
//                        each export, event and attribute
//   react/src/elements.rs the DOM's elements, React's attributes and events,
//                        and CSS properties, each event and attribute gated
//                        by the release that first has it
//
// Run it when React has a new release: `bun run generate:react`. It installs
// the latest patch of every minor release since 18.0 into target/react-versions/
// (which needs the network the first time), and reads what each one exports,
// and the events and attributes React DOM registers.
//
// `bun react/generate.ts --elements <file>` writes only elements.rs, to
// `<file>`, from the versions.json there is: what the tests check it against.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const cache = join(root, "target", "react-versions");
const FIRST = [18, 0];

// What each entry point exports, as a program imports it. The server and
// static entries differ by environment: Web streams in one, Node's in the other.
const ENTRIES = [
  "react",
  "react-dom",
  "react-dom/client",
  "react-dom/server.browser",
  "react-dom/server.node",
  "react-dom/static.browser",
  "react-dom/static.node",
];

type Since = { since: string; removed?: string };

// `bun react/generate.ts --read <dir> <NODE_ENV>`: one version's exports, in a
// process of its own, since React picks its build when it's first loaded.
if (process.argv[2] === "--read") {
  const [dir, env] = process.argv.slice(3);
  process.env.NODE_ENV = env;
  const require = createRequire(join(dir, "package.json"));
  const exports: Record<string, string[]> = {};
  for (const entry of ENTRIES) {
    try {
      exports[entry] = Object.keys(require(entry)).filter((name) => name !== "default");
    } catch {
      exports[entry] = [];
    }
  }
  console.log(JSON.stringify(exports));
  process.exit(0);
}

// `bun react/generate.ts --registry <dir>`: the events and attributes React
// DOM knows. They're internal, so this runs its development build with one
// more line that hands them out.
if (process.argv[2] === "--registry") {
  const dir = process.argv[3];
  const { GlobalRegistrator } = await import("@happy-dom/global-registrator");
  GlobalRegistrator.register();
  const require = createRequire(join(dir, "package.json"));
  const cjs = join(dir, "node_modules", "react-dom", "cjs");
  const file = [join(cjs, "react-dom-client.development.js"), join(cjs, "react-dom.development.js")].find(existsSync)!;
  let source = readFileSync(file, "utf8");
  const end = source.lastIndexOf("})();");
  source =
    source.slice(0, end) +
    "exports.__registry = { events: Object.keys(registrationNameDependencies), attributes: Object.values(possibleStandardNames) };\n" +
    source.slice(end);
  const module = { exports: {} as any };
  new Function("exports", "require", "module", "process", source)(module.exports, require, module, {
    env: { NODE_ENV: "development" },
  });
  const { events, attributes } = module.exports.__registry;
  console.log(JSON.stringify({ events: [...new Set(events)].sort(), attributes: [...new Set(attributes)].sort() }));
  process.exit(0);
}

function run(cmd: string[], cwd = root): string {
  const p = spawnSync(cmd[0], cmd.slice(1), { cwd, encoding: "utf8", maxBuffer: 64 << 20 });
  if (p.status !== 0) throw new Error(`${cmd.join(" ")} failed:\n${p.stderr}`);
  return p.stdout;
}

// ── Each minor release since 18.0 ───────────────────────────────────────

const elementsOnly = process.argv[2] === "--elements" ? process.argv[3] : null;
const releases: Record<string, string> = elementsOnly ? (await import("./versions.json")).default.releases : {};
if (!elementsOnly) {
  const registry = await (await fetch("https://registry.npmjs.org/react")).json();
  for (const version of Object.keys(registry.versions).filter((v) => /^\d+\.\d+\.\d+$/.test(v))) {
    const [major, minor] = version.split(".").map(Number);
    if (major < FIRST[0] || (major === FIRST[0] && minor < FIRST[1])) continue;
    const key = `${major}.${minor}`;
    if (!releases[key] || compare(version, releases[key]) > 0) releases[key] = version;
  }
}
const minors = Object.keys(releases).sort(compare);

function compare(a: string, b: string): number {
  const [x, y] = [a, b].map((v) => v.split(".").map(Number));
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0);
  }
  return 0;
}

type Read = { exports: Record<string, string[]>; events: string[]; attributes: string[] };
const read: Record<string, Read> = {};
for (const minor of elementsOnly ? [] : minors) {
  const version = releases[minor];
  const dir = join(cache, version);
  if (!existsSync(join(dir, "node_modules", "react-dom"))) {
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "package.json"), '{ "private": true }\n');
    run(["bun", "add", `react@${version}`, `react-dom@${version}`], dir);
  }
  // Some exports, like `act`, are only in the development build.
  const dev = JSON.parse(run(["bun", import.meta.path, "--read", dir, "development"]));
  const prod = JSON.parse(run(["bun", import.meta.path, "--read", dir, "production"]));
  const exports = Object.fromEntries(ENTRIES.map((e) => [e, [...new Set([...dev[e], ...prod[e]])].sort()]));
  const { events, attributes } = JSON.parse(run(["bun", import.meta.path, "--registry", dir]));
  read[minor] = { exports, events, attributes };
  console.log(`React ${version}: ${exports.react.length} exports, ${events.length} events, ${attributes.length} attributes`);
}

// The first release with each name, and the first one without it after that.
function since(present: (minor: string) => string[]): Record<string, Since> {
  const names = new Set(minors.flatMap(present));
  const out: Record<string, Since> = {};
  for (const name of [...names].sort()) {
    const has = minors.map((m) => present(m).includes(name));
    const first = has.indexOf(true);
    const gone = has.indexOf(false, first);
    out[name] = gone < 0 ? { since: minors[first] } : { since: minors[first], removed: minors[gone] };
  }
  return out;
}

const latest = minors.at(-1)!;
const versions = elementsOnly
  ? (await import("./versions.json")).default
  : {
      _: "Generated by react/generate.ts from each React release. Do not edit.",
      releases: Object.fromEntries(minors.map((m) => [m, releases[m]])),
      exports: Object.fromEntries(ENTRIES.map((e) => [e, since((m) => read[m].exports[e])])),
      events: since((m) => read[m].events),
      attributes: since((m) => read[m].attributes),
    };
if (!elementsOnly) writeFileSync(join(import.meta.dir, "versions.json"), `${JSON.stringify(versions, null, 1)}\n`);

// ── react/src/elements.rs ────────────────────────────────────────────────────

// Props that take only a boolean. Others React also accepts booleans for
// (`hidden`, `capture`, `download`, `value`, `draggable`) take strings too.
const BOOLEAN = new Set(
  "allowFullScreen async autoFocus autoPlay checked controls credentialless default defer disabled disablePictureInPicture disableRemotePlayback formNoValidate inert itemScope loop multiple muted noModule noValidate open playsInline readOnly required reversed scoped seamless selected".split(
    " ",
  ),
);

// Written by hand in lib.rs, where they take their own types.
const HAND_WRITTEN = new Set(["style", "dangerouslySetInnerHTML", "children", "ref", "key", "innerHTML"]);

// Which React event each handler gets, as react.dev's common components
// page groups them. Any other is `Event`, React's base event.
const EVENT_TYPES: Record<string, string> = {};
const groups: Record<string, string> = {
  Animation: "AnimationEnd AnimationIteration AnimationStart",
  Mouse: "AuxClick Click ContextMenu DoubleClick MouseDown MouseEnter MouseLeave MouseMove MouseOut MouseOver MouseUp",
  Input: "BeforeInput",
  Focus: "Blur Focus",
  Composition: "CompositionEnd CompositionStart CompositionUpdate",
  Clipboard: "Copy Cut Paste",
  Drag: "Drag DragEnd DragEnter DragExit DragLeave DragOver DragStart Drop",
  Pointer:
    "GotPointerCapture LostPointerCapture PointerCancel PointerDown PointerEnter PointerLeave PointerMove PointerOut PointerOver PointerUp",
  Keyboard: "KeyDown KeyPress KeyUp",
  Touch: "TouchCancel TouchEnd TouchMove TouchStart",
  Transition: "TransitionCancel TransitionEnd TransitionRun TransitionStart",
  Wheel: "Wheel",
  Toggle: "BeforeToggle Toggle",
  Change: "Change Input",
  Ui: "Scroll ScrollEnd",
};
for (const [type, names] of Object.entries(groups)) for (const name of names.split(" ")) EVENT_TYPES[`on${name}`] = type;

const KEYWORDS = new Set(
  "as async await box break const continue crate default do dyn else enum extern false final fn for gen if impl in let loop macro match mod move mut override priv pub ref return self static struct super trait true try type typeof unsafe unsized use virtual where while yield".split(
    " ",
  ),
);

function snake(name: string): string {
  const s = name
    .replace(/[-:]/g, "_")
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/([A-Z])([A-Z][a-z])/g, "$1_$2")
    .toLowerCase();
  return KEYWORDS.has(s) ? `r#${s}` : s;
}

function gate(entry: Since): string {
  return entry.since === minors[0] ? "" : `    #[cfg(react = "${entry.since}")]\n`;
}

const lines: string[] = [
  "// Generated by react/generate.ts from React DOM's own tables (React",
  `// ${minors.map((m) => releases[m]).join(", ")}) and W3C's specs (@webref/elements, @webref/css). Do not edit.`,
  "//",
  "// An attribute or event React DOM first knows in a later release is gated",
  '// by it, `#[cfg(react = "19.2")]` (ADR 0043).',
  "",
  "use super::*;",
  "",
  "/// Attributes, as React names them: `class_name` is `className`. Any other,",
  '/// like `aria-*` and `data-*`, is [`attr`](Element::attr).',
  "impl Element {",
];
const methods = new Set(["children", "key", "r#ref", "attr"]);
for (const [name, entry] of Object.entries(versions.attributes)) {
  if (entry.removed || HAND_WRITTEN.has(name) || name.startsWith("on")) continue;
  const method = snake(name);
  if (methods.has(method)) throw new Error(`two attributes are \`${method}\``);
  methods.add(method);
  const ty = BOOLEAN.has(name) ? "bool" : "impl Value";
  lines.push(
    `    /// \`${name}\``,
    `${gate(entry)}    #[rust_js::link_name = "prop ${name}"]`,
    `    pub fn ${method}(self, value: ${ty}) -> Element {`,
    "        unreachable!()",
    "    }",
    "",
  );
}
lines.push("}", "", "/// Event handlers: `on_click` is `onClick`. A handler must not borrow", "/// anything, since it runs later: write it `move |e| ..`.", "impl Element {");
for (const [name, entry] of Object.entries(versions.events)) {
  if (entry.removed) continue;
  const base = name.replace(/Capture$/, "");
  const type = EVENT_TYPES[base] ?? "Event";
  const method = snake(name);
  if (methods.has(method)) throw new Error(`two methods are \`${method}\``);
  methods.add(method);
  lines.push(
    `    /// \`${name}\``,
    `${gate(entry)}    #[rust_js::link_name = "prop ${name}"]`,
    `    pub fn ${method}(self, handler: impl Fn(&event::${type}) + 'static) -> Element {`,
    "        unreachable!()",
    "    }",
    "",
  );
}
lines.push("}", "");

// Elements: HTML's, and SVG's with its animations, filters and masks.
const webrefElements = await import("@webref/elements");
const specs = await (webrefElements.default ?? webrefElements).listAll();
const tags = new Set<string>();
for (const spec of ["html", "SVG2", "svg-animations", "filter-effects-1", "css-masking-1"]) {
  for (const element of specs[spec]?.elements ?? []) {
    if (!element.obsolete && /^[a-zA-Z][a-zA-Z0-9]*$/.test(element.name)) tags.add(element.name);
  }
}
lines.push("/// The DOM's elements: `div()` is `<div>`, `linear_gradient()` `<linearGradient>`.", "pub mod html {", "    use super::Element;", "", "    unsafe extern \"Rust\" {");
for (const tag of [...tags].sort()) {
  lines.push(`        /// \`<${tag}>\``, `        #[link_name = "<${tag}>"]`, `        pub safe fn ${snake(tag)}() -> Element;`);
}
lines.push("    }", "}", "");

// Style: CSS's properties, as React names them, `backgroundColor`.
const webrefCss = await import("@webref/css");
const css = await (webrefCss.default ?? webrefCss).listAll();
const properties = [...new Set(css.properties.map((p: { name: string }) => p.name))]
  .filter((name) => !name.startsWith("-"))
  .sort() as string[];
lines.push("/// CSS properties: `background_color` is `backgroundColor`. A number is in", "/// pixels where CSS needs a unit, as React makes it.", "impl Style {");
const styleMethods = new Set(["new", "set"]);
for (const property of properties) {
  const camel = property.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const method = snake(camel);
  if (styleMethods.has(method)) continue;
  styleMethods.add(method);
  lines.push(
    `    /// \`${property}\``,
    `    #[rust_js::link_name = "prop ${camel}"]`,
    `    pub fn ${method}(self, value: impl Value) -> Style {`,
    "        unreachable!()",
    "    }",
    "",
  );
}
lines.push("}");
writeFileSync(elementsOnly ?? join(import.meta.dir, "src", "elements.rs"), `${lines.join("\n")}\n`);
console.log(
  `react/versions.json: ${minors.length} releases, ${latest} latest; react/src/elements.rs: ${methods.size} element methods, ${tags.size} elements, ${styleMethods.size} style properties`,
);
