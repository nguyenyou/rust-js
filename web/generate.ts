// Generate src/lib.rs, the `web` crate, from W3C's WebIDL (ADR 0024).
//
//   cd web && bun install && bun generate.ts
//
// Every rule about what gets in is in this file: which interfaces
// (INTERFACES), how WebIDL types become Rust (`rustType`), and the names.
// A member is generated only if rust-js supports all of its types; the rest
// are counted and skipped, and rerunning after rust-js grows picks them up.

import idl from "@webref/idl";
import webref from "@webref/idl/package.json" with { type: "json" };

// The specs to read. Partial interfaces and mixins from these are merged in.
const SPECS = ["dom", "html", "uievents", "pointerevents", "cssom", "cssom-view", "geometry", "fetch"];

// The everyday DOM. Members that use any other interface are skipped.
const INTERFACES = [
  // dom
  "EventTarget", "Event", "Node", "CharacterData", "Text", "Comment", "Element", "Document",
  "DocumentFragment", "DOMTokenList", "NodeList", "HTMLCollection",
  // html
  "HTMLElement", "HTMLAnchorElement", "HTMLButtonElement", "HTMLDivElement", "HTMLFormElement",
  "HTMLHeadingElement", "HTMLImageElement", "HTMLInputElement", "HTMLLabelElement", "HTMLLIElement",
  "HTMLOListElement", "HTMLOptionElement", "HTMLOutputElement", "HTMLParagraphElement",
  "HTMLSelectElement", "HTMLSpanElement", "HTMLTextAreaElement", "HTMLUListElement",
  "Window", "Location", "History", "Storage",
  // uievents
  "UIEvent", "FocusEvent", "MouseEvent", "KeyboardEvent", "InputEvent",
  // cssom
  "CSSStyleDeclaration", "CSSStyleProperties",
  // cssom-view, geometry: where things are on the page
  "DOMRectReadOnly", "DOMRect",
  // fetch: `window::fetch`, and what it gives back
  "Headers", "Request", "Response",
];
const known = new Set(INTERFACES);

// JS's own types that WebIDL uses, declared by hand at the crate root.
const BUILTINS = new Set(["ArrayBuffer", "Uint8Array"]);

// The globals at the crate root: `document`, `window`.
const GLOBALS: [string, string][] = [["document", "Document"], ["window", "Window"]];

// ── Reading the IDL ─────────────────────────────────────────────────────

type IdlType = { idlType: string | IdlType[]; nullable: boolean; union: boolean; generic: string };
type Arg = { name: string; idlType: IdlType; optional: boolean; variadic: boolean };
type Member = {
  type: string;
  name?: string;
  special?: string;
  readonly?: boolean;
  idlType?: IdlType;
  arguments?: Arg[];
  extAttrs?: { name: string }[];
};
type Def = {
  type: string;
  name: string;
  partial?: boolean;
  inheritance?: string;
  members?: Member[];
  target?: string;
  includes?: string;
  idlType?: IdlType;
  extAttrs?: { name: string }[];
};

const all: Record<string, Def[]> = await idl.parseAll();
const read: Def[] = SPECS.flatMap((spec) => all[spec]);
const everywhere: Def[] = Object.values(all).flat();

// Names that are strings (enums) or other types (typedefs), from any spec.
const enums = new Set(everywhere.filter((d) => d.type === "enum").map((d) => d.name));
const typedefs = new Map(everywhere.filter((d) => d.type === "typedef").map((d) => [d.name, d.idlType!]));

type Interface = { name: string; parent?: string; members: { member: Member; from: string }[]; constructible: boolean };
const interfaces = new Map<string, Interface>();
const mixins = new Map<string, Member[]>();
const includes = new Map<string, string[]>();

for (const d of read) {
  if (d.type === "interface" && known.has(d.name)) {
    const i = interfaces.get(d.name) ?? { name: d.name, members: [], constructible: false };
    if (!d.partial) {
      i.parent = d.inheritance ?? undefined;
      // `[HTMLConstructor]` elements can't be made with `new`.
      i.constructible = !(d.extAttrs ?? []).some((a) => a.name === "HTMLConstructor");
    }
    i.members.push(...(d.members ?? []).map((member) => ({ member, from: d.name })));
    interfaces.set(d.name, i);
  } else if (d.type === "interface mixin") {
    mixins.set(d.name, [...(mixins.get(d.name) ?? []), ...(d.members ?? [])]);
  } else if (d.type === "includes") {
    includes.set(d.target!, [...(includes.get(d.target!) ?? []), d.includes!]);
  }
}
for (const [target, names] of includes) {
  const i = interfaces.get(target);
  for (const name of names) i?.members.push(...(mixins.get(name) ?? []).map((member) => ({ member, from: target })));
}
const missing = INTERFACES.filter((name) => !interfaces.has(name));
if (missing.length) throw new Error(`not in ${SPECS.join(", ")}: ${missing.join(", ")}`);

// ── Names ───────────────────────────────────────────────────────────────

/** `HTMLInputElement` → `["HTML", "Input", "Element"]`, `innerHTML` → `["inner", "HTML"]`. */
const words = (name: string) => name.match(/[A-Z]+(?![a-z])|[A-Z]?[a-z0-9]+/g) ?? [name];

/** Types, web-sys style: `HTMLInputElement` → `HtmlInputElement`. */
const typeName = (name: string) => words(name).map((w) => w[0].toUpperCase() + w.slice(1).toLowerCase()).join("");

const KEYWORDS = new Set(
  ("as async await box break const continue crate do dyn else enum extern false final fn for gen if impl in " +
    "let loop macro match mod move mut override priv pub ref return self Self static struct super trait true " +
    "try type typeof unsafe unsized use virtual where while yield abstract become").split(" "),
);

/** `getElementById` → `get_element_by_id`. */
const snakeWords = (name: string) => words(name).map((w) => w.toLowerCase()).join("_");

/** Functions, modules and parameters, with a `_` after a Rust keyword: `type` → `type_`. */
const snake = (name: string) => {
  const s = snakeWords(name);
  return KEYWORDS.has(s) ? `${s}_` : s;
};

// ── Types ───────────────────────────────────────────────────────────────

const STRINGS = new Set(["DOMString", "USVString", "CSSOMString", "ByteString"]);
const NUMBERS: Record<string, string> = {
  boolean: "bool", byte: "i8", octet: "u8", short: "i16", "unsigned short": "u16",
  long: "i32", "unsigned long": "u32", double: "f64", "unrestricted double": "f64",
};

type Position = "param" | "result";

/** The Rust type for a (non-union) WebIDL type, or why there isn't one. */
function rustType(t: IdlType, at: Position): string | { skip: string } {
  if (t.union) return { skip: "union" };
  // A promise a function returns is `.await`ed in Rust (ADR 0029).
  if (t.generic === "Promise" && at === "result") {
    const inner = rustType((t.idlType as IdlType[])[0], "result");
    return typeof inner === "string" ? `Promise<${inner}>` : inner;
  }
  if (t.generic) return { skip: t.generic };
  const name = t.idlType as string;
  const aliased = typedefs.get(name);
  // (webidl2 types are objects with getters: pass them on as they are.)
  if (aliased) return rustType(aliased, at);
  if (name === "undefined") return at === "result" ? "()" : { skip: "undefined parameter" };
  if (NUMBERS[name]) return NUMBERS[name];
  if (STRINGS.has(name) || enums.has(name)) return at === "param" ? "&str" : "String";
  if (name === "EventListener" && at === "param") return "Box<dyn FnMut(&Event)>";
  if (known.has(name) || BUILTINS.has(name)) return at === "param" ? `&${typeName(name)}` : `&'static ${typeName(name)}`;
  return { skip: name };
}

/** The Rust types a parameter can take: one per supported member of a union. */
function alternatives(t: IdlType): string[] {
  // A typedef of a union, like `RequestInfo`, is that union.
  const aliased = !t.union && !t.generic && typedefs.get(t.idlType as string);
  if (aliased) return alternatives(aliased);
  const options = t.union ? (t.idlType as IdlType[]) : [t];
  return options.map((o) => rustType(o, "param")).filter((r): r is string => typeof r === "string");
}

/** `&str` → `str`, `&HtmlElement` → `html_element`: for `append_with_str`. */
const suffix = (rust: string) => snake(rust.replace(/^&/, "").replace(/^Box<dyn FnMut.*$/, "listener"));

// ── Generating ──────────────────────────────────────────────────────────

const skipped = new Map<string, number>();
const skip = (why: string) => skipped.set(why, (skipped.get(why) ?? 0) + 1);

type Fn = { name: string; jsName: string; params: string[]; result: string; doc: string[] };

/** The root of `name`'s inheritance chain within INTERFACES. */
function root(name: string): string {
  const parent = interfaces.get(name)!.parent;
  return parent && known.has(parent) ? root(parent) : name;
}

function mdn(iface: string, member?: string) {
  return `https://developer.mozilla.org/docs/Web/API/${iface}${member ? `/${member}` : ""}`;
}

function functionsOf(i: Interface): Fn[] {
  const fns: Fn[] = [];
  const self = `this: &${typeName(i.name)}`;
  const nullable = (t: IdlType) => t.nullable || typedefs.get(t.idlType as string)?.nullable;
  const nullNote = "May be `null` in JS, which this binding doesn't say yet (ADR 0024).";

  // Arguments up to the first optional one, as lists of Rust types to try.
  const signatures = (args: Arg[]): { names: string[]; options: string[][] } | { skip: string } => {
    const names: string[] = [];
    const options: string[][] = [];
    for (const a of args) {
      if (a.optional) break;
      const alts = alternatives(a.idlType);
      if (alts.length === 0) {
        const why = rustType(a.idlType, "param");
        return { skip: typeof why === "string" ? "?" : why.skip };
      }
      names.push(snake(a.name));
      options.push(alts);
    }
    return { names, options };
  };

  // Each union parameter's alternatives make their own function: the first
  // keeps the name, the others add `_with_<type>`. Only the first union
  // varies; any others take their first alternative.
  const variants = (base: string, sig: { names: string[]; options: string[][] }) => {
    const varying = sig.options.findIndex((o) => o.length > 1);
    const pick = (k: number) => sig.options.map((o, j) => (j === varying ? o[k] : o[0]));
    const count = varying < 0 ? 1 : sig.options[varying].length;
    return Array.from({ length: count }, (_, k) => ({
      name: k === 0 ? base : `${base}_with_${suffix(sig.options[varying][k])}`,
      params: pick(k).map((ty, j) => `${sig.names[j]}: ${ty}`),
    }));
  };

  for (const { member: m } of i.members) {
    if (m.type === "constructor") {
      if (!i.constructible) continue;
      const sig = signatures(m.arguments ?? []);
      if ("skip" in sig) {
        skip(sig.skip);
        continue;
      }
      for (const v of variants("new", sig)) {
        fns.push({ name: v.name, jsName: `new ${i.name}`, params: v.params, result: `&'static ${typeName(i.name)}`, doc: [`[MDN](${mdn(i.name, i.name)})`] });
      }
    } else if (m.type === "attribute") {
      if (m.special === "static") {
        skip("static");
        continue;
      }
      const result = rustType(m.idlType!, "result");
      if (typeof result !== "string") {
        skip(result.skip);
        continue;
      }
      const doc = [`[MDN](${mdn(i.name, m.name)})`];
      fns.push({ name: snake(m.name!), jsName: `get ${m.name}`, params: [self], result, doc: nullable(m.idlType!) ? [...doc, nullNote] : doc });
      const forwards = (m.extAttrs ?? []).some((a) => a.name === "PutForwards" || a.name === "Replaceable");
      const value = alternatives(m.idlType!)[0];
      if (!m.readonly && !forwards && value) {
        fns.push({ name: `set_${snakeWords(m.name!)}`, jsName: `set ${m.name}`, params: [self, `value: ${value}`], result: "()", doc });
      }
    } else if (m.type === "operation") {
      if (!m.name || m.special === "static") {
        skip(m.special || "unnamed");
        continue;
      }
      const result = rustType(m.idlType!, "result");
      const sig = signatures(m.arguments ?? []);
      if (typeof result !== "string" || "skip" in sig) {
        skip(typeof result !== "string" ? result.skip : (sig as { skip: string }).skip);
        continue;
      }
      const doc = [`[MDN](${mdn(i.name, m.name)})`];
      for (const v of variants(snake(m.name), sig)) {
        fns.push({ name: v.name, jsName: m.name, params: [self, ...v.params], result, doc: nullable(m.idlType!) ? [...doc, nullNote] : doc });
      }
    }
  }

  // An unchecked cast from the root of the chain: `html_input_element::unchecked_from(e)`.
  if (root(i.name) !== i.name) {
    fns.push({
      name: "unchecked_from",
      jsName: "this",
      params: [`this: &${typeName(root(i.name))}`],
      result: `&'static ${typeName(i.name)}`,
      doc: [`Treats \`this\` as \`${typeName(i.name)}\` without checking that it is one.`],
    });
  }

  // The first of each name wins: later overloads and clashes are skipped.
  const seen = new Set<string>();
  return fns.filter((f) => {
    if (seen.has(f.name)) {
      skip("overload or clash");
      return false;
    }
    seen.add(f.name);
    return true;
  });
}

const out: string[] = [];
const line = (s = "") => out.push(s);

line(`//! The web platform for rust-js: DOM bindings generated by \`web/generate.ts\``);
line(`//! from W3C's WebIDL (\`@webref/idl\` ${webref.version}; specs: ${SPECS.join(", ")}). Do not edit.`);
line(`//!`);
line(`//! Each interface is a type (\`Element\`) and a module of its members`);
line(`//! (\`element::append\`). Inheritance is \`Deref\`, so an \`&HtmlButtonElement\``);
line(`//! goes wherever an \`&Element\` or \`&Node\` is expected. See ADR 0024.`);
line();
line(`#![feature(extern_types)]`);
line(`// Many Rust functions call the same JS name: an overload per union member
// (\`before\`, \`before_with_str\`), and methods of the same name on different
// interfaces. rustc warns because in native code they would be one symbol.`);
line(`#![allow(clashing_extern_declarations)]`);
line();
line(`use core::marker::PhantomData;`);
line(`use core::ops::Deref;`);
line();
line(`unsafe extern "Rust" {`);
line(`    /// Any JS object. Every type below holds a \`PhantomData\` of it, which is`);
line(`    /// how rust-js knows it's a JS object.`);
line(`    pub type JsObject;`);
for (const [name, type] of GLOBALS) {
  line();
  line(`    /// The \`${name}\` global.`);
  line(`    pub safe static ${name}: &'static ${type};`);
}
line(`}`);
line();
line(`/// A JS [\`Promise\`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Promise)
/// of a \`T\`. \`.await\` on one is JS's \`await\`; a rejected one throws, like a
/// panic. See ADR 0029.
pub struct Promise<T>(PhantomData<JsObject>, PhantomData<T>);

impl<T> core::future::Future for Promise<T> {
    type Output = T;

    fn poll(self: core::pin::Pin<&mut Self>, _: &mut core::task::Context<'_>) -> core::task::Poll<T> {
        unreachable!("rust-js compiles \`.await\` to JS's \`await\`")
    }
}

unsafe extern "Rust" {
    /// Run a future without waiting for it, as from an event handler:
    /// \`spawn(Box::new(async move { .. }))\`. A JS promise is already
    /// running, so in JS this is the promise itself, left unawaited.
    #[link_name = "this"]
    pub safe fn spawn(this: Box<dyn core::future::Future<Output = ()>>);
}

/// A JS [\`ArrayBuffer\`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/ArrayBuffer):
/// raw bytes, as \`response::array_buffer\` gives them.
pub struct ArrayBuffer(PhantomData<JsObject>);

pub mod array_buffer {
    use super::*;

    unsafe extern "Rust" {
        #[link_name = "get byteLength"]
        pub safe fn byte_length(this: &ArrayBuffer) -> u32;
    }
}

/// A JS [\`Uint8Array\`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array):
/// a view of the bytes in an \`ArrayBuffer\`, as \`response::bytes\` gives them.
pub struct Uint8Array(PhantomData<JsObject>);

pub mod uint8_array {
    use super::*;

    unsafe extern "Rust" {
        /// A view of all of \`buffer\`.
        #[link_name = "new Uint8Array"]
        pub safe fn new(buffer: &ArrayBuffer) -> &'static Uint8Array;

        /// How many bytes it views.
        #[link_name = "get length"]
        pub safe fn length(this: &Uint8Array) -> u32;

        /// The buffer it views.
        #[link_name = "get buffer"]
        pub safe fn buffer(this: &Uint8Array) -> &'static ArrayBuffer;
    }
}`);

let count = 0;
for (const name of INTERFACES) {
  const i = interfaces.get(name)!;
  const type = typeName(name);
  line();
  line(`/// [\`${name}\`](${mdn(name)})`);
  line(`pub struct ${type}(PhantomData<JsObject>);`);
  if (i.parent && known.has(i.parent)) {
    const parent = typeName(i.parent);
    line();
    line(`impl Deref for ${type} {`);
    line(`    type Target = ${parent};`);
    line();
    line(`    fn deref(&self) -> &${parent} {`);
    line(`        // Never runs: rust-js compiles this \`Deref\` to the object itself.`);
    line(`        unsafe { &*(self as *const Self as *const ${parent}) }`);
    line(`    }`);
    line(`}`);
  }
  const fns = functionsOf(i);
  if (fns.length === 0) continue;
  count += fns.length;
  line();
  line(`pub mod ${snake(name)} {`);
  line(`    use super::*;`);
  line();
  line(`    unsafe extern "Rust" {`);
  fns.forEach((f, k) => {
    if (k > 0) line();
    for (const d of f.doc) line(`        /// ${d}`);
    if (f.jsName !== f.name) line(`        #[link_name = ${JSON.stringify(f.jsName)}]`);
    const result = f.result === "()" ? "" : ` -> ${f.result}`;
    line(`        pub safe fn ${f.name}(${f.params.join(", ")})${result};`);
  });
  line(`    }`);
  line(`}`);
}

await Bun.write(new URL("./src/lib.rs", import.meta.url), `${out.join("\n")}\n`);
const reasons = [...skipped].sort((a, b) => b[1] - a[1]).map(([why, n]) => `${why} ${n}`);
console.log(`src/lib.rs: ${INTERFACES.length} interfaces, ${count} functions`);
console.log(`skipped: ${reasons.slice(0, 12).join(", ")}${reasons.length > 12 ? ", ..." : ""}`);
