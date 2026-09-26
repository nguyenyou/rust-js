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
const SPECS = ["dom", "html", "uievents", "pointerevents", "cssom", "cssom-view", "geometry", "fetch", "encoding", "wasm-js-api", "wasm-web-api"];

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
  "HTMLTableElement", "HTMLTableSectionElement", "HTMLTableRowElement", "HTMLTableCellElement",
  "Window", "Location", "History", "Storage",
  // uievents
  "UIEvent", "FocusEvent", "MouseEvent", "KeyboardEvent", "InputEvent",
  // cssom
  "CSSStyleDeclaration", "CSSStyleProperties",
  // cssom-view, geometry: where things are on the page
  "DOMRectReadOnly", "DOMRect",
  // fetch: `window::fetch`, and what it gives back
  "Headers", "Request", "Response",
  // encoding: text to bytes and back
  "TextEncoder", "TextDecoder",
  // wasm-js-api: `WebAssembly.Module` and friends
  "Module", "Instance", "Memory",
];
const known = new Set(INTERFACES);

// Namespaces: a module of functions, like `web_assembly::compile`.
const NAMESPACES = ["WebAssembly"];

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
  required?: boolean;
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
  extAttrs?: { name: string; rhs?: { value: string } }[];
};

const all: Record<string, Def[]> = await idl.parseAll();
const read: Def[] = SPECS.flatMap((spec) => all[spec]);
const everywhere: Def[] = Object.values(all).flat();

// Names that are strings (enums) or other types (typedefs), from any spec.
const enums = new Set(everywhere.filter((d) => d.type === "enum").map((d) => d.name));
const typedefs = new Map(everywhere.filter((d) => d.type === "typedef").map((d) => [d.name, d.idlType!]));
const dictionaries = new Map(everywhere.filter((d) => d.type === "dictionary" && !d.partial).map((d) => [d.name, d]));

type Interface = {
  name: string;
  parent?: string;
  members: { member: Member; from: string }[];
  constructible: boolean;
  /** `[LegacyNamespace=WebAssembly]`: JS calls it `WebAssembly.Module`. */
  legacyNamespace?: string;
  /** A `namespace`: functions, and no type. */
  isNamespace?: boolean;
};
const interfaces = new Map<string, Interface>();
const namespaces = new Map<string, Interface>();
const mixins = new Map<string, Member[]>();
const includes = new Map<string, string[]>();

for (const d of read) {
  if (d.type === "interface" && known.has(d.name)) {
    const i = interfaces.get(d.name) ?? { name: d.name, members: [], constructible: false };
    if (!d.partial) {
      i.parent = d.inheritance ?? undefined;
      // `[HTMLConstructor]` elements can't be made with `new`.
      i.constructible = !(d.extAttrs ?? []).some((a) => a.name === "HTMLConstructor");
      i.legacyNamespace = (d.extAttrs ?? []).find((a) => a.name === "LegacyNamespace")?.rhs?.value;
    }
    i.members.push(...(d.members ?? []).map((member) => ({ member, from: d.name })));
    interfaces.set(d.name, i);
  } else if (d.type === "namespace" && NAMESPACES.includes(d.name)) {
    const n = namespaces.get(d.name) ?? { name: d.name, members: [], constructible: false, isNamespace: true };
    n.members.push(...(d.members ?? []).map((member) => ({ member, from: d.name })));
    namespaces.set(d.name, n);
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

/** An interface's name with its namespace, if it has one: `WebAssemblyModule`. */
const qualified = (name: string) => (interfaces.get(name)?.legacyNamespace ?? "") + name;

/** What JS calls an interface: `WebAssembly.Module`, `Element`. */
const jsName = (i: Interface) => (i.legacyNamespace ? `${i.legacyNamespace}.${i.name}` : i.name);

/** Types, web-sys style: `HTMLInputElement` → `HtmlInputElement`, `Module` → `WebAssemblyModule`. */
const typeName = (name: string) => words(qualified(name)).map((w) => w[0].toUpperCase() + w.slice(1).toLowerCase()).join("");

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
  // A promise a function returns is `.await`ed in Rust (ADR 0029). One it
  // takes is passed as it is: `compile_streaming(window::fetch_with_str(..))`.
  if (t.generic === "Promise") {
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
  // Any JS object: a Rust value of any type in, an opaque object out.
  if (name === "object") return at === "param" ? "&dyn core::any::Any" : "&'static JsObject";
  const dictionary = dictionaries.get(name);
  if (dictionary && at === "result") return dictionaryType(dictionary);
  if (known.has(name) || BUILTINS.has(name)) return at === "param" ? `&${typeName(name)}` : `&'static ${typeName(name)}`;
  return { skip: name };
}

/** The fields of each dictionary a result uses, as `(name, type)`. */
const usedDictionaries = new Map<string, [string, string][]>();

/**
 * A dictionary a function returns is a Rust struct, which rust-js makes a
 * plain JS object (ADR 0020): its fields are read as they are. Only when
 * every field is required, of a supported type, and named the same in Rust.
 */
function dictionaryType(d: Def): string | { skip: string } {
  const fields: [string, string][] = [];
  for (const m of (d.members ?? []) as Member[]) {
    const rust = rustType(m.idlType!, "result");
    if (d.inheritance || !m.required || snake(m.name!) !== m.name || typeof rust !== "string") return { skip: d.name };
    fields.push([m.name!, rust]);
  }
  usedDictionaries.set(d.name, fields);
  return typeName(d.name);
}

/** The Rust types a parameter can take: one per supported member of a union. */
function alternatives(t: IdlType): string[] {
  // A typedef of a union, like `RequestInfo`, is that union, and a union
  // inside a union (`ArrayBufferView` in `BufferSource`) is its members.
  const aliased = !t.union && !t.generic && typedefs.get(t.idlType as string);
  if (aliased) return alternatives(aliased);
  if (t.union) return (t.idlType as IdlType[]).flatMap(alternatives);
  const rust = rustType(t, "param");
  return typeof rust === "string" ? [rust] : [];
}

/** Is `t` a union, written out or through a typedef? */
function isUnion(t: IdlType): boolean {
  const aliased = !t.union && !t.generic && typedefs.get(t.idlType as string);
  return t.union || (!!aliased && isUnion(aliased));
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
  const ns = interfaces.get(iface)?.legacyNamespace;
  const page = ns ? `JavaScript/Reference/Global_Objects/${ns}/${iface}` : NAMESPACES.includes(iface) ? `JavaScript/Reference/Global_Objects/${iface}` : `API/${iface}`;
  return `https://developer.mozilla.org/docs/Web/${page}${member ? `/${member}` : ""}`;
}

function functionsOf(i: Interface): Fn[] {
  const fns: Fn[] = [];
  // A namespace's functions are called on it: `WebAssembly.compile(bytes)`.
  const self = i.isNamespace ? [] : [`this: &${typeName(i.name)}`];
  const member = (name: string) => (i.isNamespace ? `${i.name}.${name}` : name);
  // A result that may be `null` is an `Option`: `None` in Rust (ADR 0030).
  const nullable = (t: IdlType) => t.nullable || typedefs.get(t.idlType as string)?.nullable;
  const orNull = (rust: string, t: IdlType) => (nullable(t) ? `Option<${rust}>` : rust);

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
  // Each optional argument, in order, gives one more form, after the
  // required ones: `encode_with_input(this, input)`. One that's a union
  // gives a form per member, named after its type: `decode_with_uint8_array`.
  // Later ones add `_and_<name>`. The first unsupported one ends them.
  const optionalForms = (base: string, lead: string[], sig: { names: string[]; options: string[][] }, args: Arg[]) => {
    const forms: { name: string; params: string[] }[] = [];
    const params = sig.options.map((o, j) => `${sig.names[j]}: ${o[0]}`);
    const words: string[] = [];
    for (const a of args.filter((a) => a.optional)) {
      const alts = alternatives(a.idlType);
      if (alts.length === 0) break;
      const union = isUnion(a.idlType);
      const word = (alt: string) => (union ? suffix(alt) : snakeWords(a.name));
      for (const alt of union ? alts : alts.slice(0, 1)) {
        forms.push({ name: `${base}_with_${[...lead, ...words, word(alt)].join("_and_")}`, params: [...params, `${snake(a.name)}: ${alt}`] });
      }
      words.push(word(alts[0]));
      params.push(`${snake(a.name)}: ${alts[0]}`);
    }
    return forms;
  };

  const variants = (base: string, sig: { names: string[]; options: string[][] }) => {
    const varying = sig.options.findIndex((o) => o.length > 1);
    const pick = (k: number) => sig.options.map((o, j) => (j === varying ? o[k] : o[0]));
    const count = varying < 0 ? 1 : sig.options[varying].length;
    return Array.from({ length: count }, (_, k) => ({
      name: k === 0 ? base : `${base}_with_${suffix(sig.options[varying][k])}`,
      params: pick(k).map((ty, j) => `${sig.names[j]}: ${ty}`),
    }));
  };

  for (const [index, { member: m }] of i.members.entries()) {
    if (m.type === "constructor") {
      if (!i.constructible) continue;
      const sig = signatures(m.arguments ?? []);
      if ("skip" in sig) {
        skip(sig.skip);
        continue;
      }
      for (const v of [...variants("new", sig), ...optionalForms("new", [], sig, m.arguments ?? [])]) {
        fns.push({ name: v.name, jsName: `new ${jsName(i)}`, params: v.params, result: `&'static ${typeName(i.name)}`, doc: [`[MDN](${mdn(i.name, i.name)})`] });
      }
    } else if (m.type === "attribute") {
      if (m.special === "static" || i.isNamespace) {
        skip(m.special || "namespace attribute");
        continue;
      }
      const result = rustType(m.idlType!, "result");
      if (typeof result !== "string") {
        skip(result.skip);
        continue;
      }
      const doc = [`[MDN](${mdn(i.name, m.name)})`];
      fns.push({ name: snake(m.name!), jsName: `get ${m.name}`, params: self, result: orNull(result, m.idlType!), doc });
      const forwards = (m.extAttrs ?? []).some((a) => a.name === "PutForwards" || a.name === "Replaceable");
      const value = alternatives(m.idlType!)[0];
      if (!m.readonly && !forwards && value) {
        fns.push({ name: `set_${snakeWords(m.name!)}`, jsName: `set ${m.name}`, params: [...self, `value: ${value}`], result: "()", doc });
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
      // A later overload is named, as web-sys does, after the required
      // arguments that set it apart from the first: by name where the first
      // has none there (`set_range_text_with_start_and_end`), by type where
      // the types differ (`instantiate_with_web_assembly_module`).
      const first = i.members.slice(0, index).find(({ member: o }) => o.type === "operation" && o.name === m.name && o.special !== "static");
      const firstRequired = (first?.member.arguments ?? []).filter((a) => !a.optional);
      const required = (m.arguments ?? []).filter((a) => !a.optional);
      const key = (a: Arg) => JSON.stringify(a.idlType.idlType);
      const lead = !first
        ? []
        : required.flatMap((a, j) =>
            !firstRequired[j] ? [snake(a.name)] : key(firstRequired[j]) !== key(a) ? [suffix(sig.options[j][0])] : [],
          );
      const base = lead.length > 0 ? `${snake(m.name)}_with_${lead.join("_and_")}` : snake(m.name);
      for (const v of [...variants(base, sig), ...optionalForms(snake(m.name), lead, sig, m.arguments ?? [])]) {
        fns.push({ name: v.name, jsName: member(m.name), params: [...self, ...v.params], result: orNull(result, m.idlType!), doc });
      }
    }
  }

  // An unchecked cast from the root of the chain: `html_input_element::unchecked_from(e)`.
  if (!i.isNamespace && root(i.name) !== i.name) {
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

/// Whatever a JS function threw, or a promise rejected with: usually an
/// [\`Error\`](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Error).
/// An \`extern\` function that returns \`Result<T, &JsError>\` catches it (ADR 0035).
pub struct JsError(PhantomData<JsObject>);

pub mod js_error {
    use super::*;

    unsafe extern "Rust" {
        /// \`String(e)\`: an \`Error\`'s name and message, or any value as text.
        #[link_name = "String"]
        pub safe fn to_string(error: &JsError) -> String;
    }
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

/** `pub mod <name> { .. }`, holding a type's or a namespace's functions. */
function module(name: string, fns: Fn[]) {
  if (fns.length === 0) return;
  count += fns.length;
  line();
  line(`pub mod ${name} {`);
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

for (const name of INTERFACES) {
  const i = interfaces.get(name)!;
  const type = typeName(name);
  line();
  line(`/// [\`${jsName(i)}\`](${mdn(name)})`);
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
  module(snake(qualified(name)), functionsOf(i));
}

for (const name of NAMESPACES) {
  line();
  line(`/// The [\`${name}\`](${mdn(name)}) namespace.`);
  module(snake(name), functionsOf(namespaces.get(name)!));
}

// The dictionaries results use, as plain structs: JS objects (ADR 0020).
for (const [name, fields] of usedDictionaries) {
  line();
  line(`/// The \`${name}\` dictionary: a JS object with these fields.`);
  line(`pub struct ${typeName(name)} {`);
  for (const [field, type] of fields) line(`    pub ${field}: ${type},`);
  line(`}`);
}

await Bun.write(new URL("./src/lib.rs", import.meta.url), `${out.join("\n")}\n`);
const reasons = [...skipped].sort((a, b) => b[1] - a[1]).map(([why, n]) => `${why} ${n}`);
console.log(`src/lib.rs: ${INTERFACES.length} interfaces, ${NAMESPACES.length} namespaces, ${count} functions`);
console.log(`skipped: ${reasons.slice(0, 12).join(", ")}${reasons.length > 12 ? ", ..." : ""}`);
