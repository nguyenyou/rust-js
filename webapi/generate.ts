// Generate src/lib.rs, the `webapi` crate, from W3C's WebIDL (ADR 0024).
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
const SPECS = ["dom", "html", "hr-time", "uievents", "pointerevents", "cssom", "cssom-view", "geometry", "fetch", "encoding", "wasm-js-api", "wasm-web-api", "xhr", "streams", "touch-events"];

// The everyday DOM. Members that use any other interface are skipped.
const INTERFACES = [
  // dom
  "EventTarget", "Event", "Node", "CharacterData", "Text", "Comment", "Element", "Document",
  "DocumentFragment", "DOMTokenList", "NodeList", "HTMLCollection", "AbortController", "AbortSignal",
  // html
  "HTMLElement", "HTMLAnchorElement", "HTMLButtonElement", "HTMLDivElement", "HTMLFormElement",
  "HTMLHeadingElement", "HTMLImageElement", "HTMLInputElement", "HTMLLabelElement", "HTMLLIElement",
  "HTMLOListElement", "HTMLOptionElement", "HTMLOutputElement", "HTMLParagraphElement",
  "HTMLSelectElement", "HTMLSpanElement", "HTMLTextAreaElement", "HTMLUListElement",
  "HTMLTableElement", "HTMLTableSectionElement", "HTMLTableRowElement", "HTMLTableCellElement", "HTMLIFrameElement",
  "HTMLCanvasElement",
  "Window", "Location", "History", "Storage", "DataTransfer", "ToggleEvent", "MessageEvent",
  // hr-time: `window.performance`, the page's clock
  "Performance",
  // uievents
  "UIEvent", "FocusEvent", "MouseEvent", "KeyboardEvent", "InputEvent",
  // cssom
  "CSSStyleDeclaration", "CSSStyleProperties",
  // cssom-view, geometry: where things are on the page
  "DOMRectReadOnly", "DOMRect", "MediaQueryList", "MediaQueryListEvent",
  // fetch: `window::fetch`, and what it gives back
  "Headers", "Request", "Response",
  // xhr, streams, touch-events: what React's forms, server rendering and touch events use
  "FormData", "ReadableStream", "Touch", "TouchList",
  // encoding: text to bytes and back
  "TextEncoder", "TextDecoder",
  // wasm-js-api: `WebAssembly.Module` and friends
  "Module", "Instance", "Memory",
];
const known = new Set(INTERFACES);

// Members written by hand, for results the generator can't type: a union
// with an interface it doesn't bind.
const EXTRA: Record<string, Fn[]> = {
  MessageEvent: [
    {
      name: "source",
      jsName: "get source",
      params: ["this: &MessageEvent"],
      result: "Option<&'static JsObject>",
      doc: [
        "[MDN](https://developer.mozilla.org/docs/Web/API/MessageEvent/source): what sent it, a window,",
        "a `MessagePort` or a `ServiceWorker`, as an object: `js::object::is` tells which.",
      ],
    },
  ],
  FormData: [
    {
      name: "get",
      jsName: "get",
      params: ["this: &FormData", "name: &str"],
      result: "Option<String>",
      doc: [
        "[MDN](https://developer.mozilla.org/docs/Web/API/FormData/get): a text field's value.",
        "A file field's value is a `File`, which this doesn't bind.",
      ],
    },
    {
      name: "get_all",
      jsName: "getAll",
      params: ["this: &FormData", "name: &str"],
      result: "Vec<String>",
      doc: ["[MDN](https://developer.mozilla.org/docs/Web/API/FormData/getAll): every text value of a field."],
    },
  ],
};

// Namespaces: a module of functions, like `web_assembly::compile`.
const NAMESPACES = ["WebAssembly"];

// JS's own types that WebIDL uses, declared by hand at the crate root.
const BUILTINS = new Set(["ArrayBuffer", "Uint8Array"]);

// ReScript's webapi, where WebIDL has several forms of one function (ADR
// 0102): the one most programs call gets the plain name, and the others
// say what sets them apart. By interface and member, the form's type:
// `fetch(url)`, and `fetch_with_request(request)`.
const PRIMARY: Record<string, string> = { "Window.fetch": "str", "Request.constructor": "str" };

// Names a type can't give, as ReScript's: `Request.fromURL`.
const RENAMES: Record<string, string> = {
  "request::from_str": "from_url",
  "request::from_str_with_init": "from_url_with_init",
};

// A dictionary field's type, where WebIDL's is one Rust can't take: a
// `HeadersInit` is a sequence or a record, and `fetch` takes a `Headers` too.
const FIELD_TYPES: Record<string, string> = { "RequestInit.headers": "&'a Headers" };

// The globals at the crate root: `document`, `window`, and `performance`,
// which a worker has too.
const GLOBALS: [string, string][] = [["document", "Document"], ["window", "Window"], ["performance", "Performance"]];

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

const all = await idl.parseAll() as Record<string, Def[]>;
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
// "HTML" then a one-letter word (`HTMLIFrameElement`) would read as "HTMLI":
// split it off first, as web-sys does (`HtmlIFrameElement`).
const words = (name: string) =>
  name.replace(/^HTML(?=[A-Z][A-Z])/, "Html").match(/[A-Z]+(?![a-z])|[A-Z]?[a-z0-9]+/g) ?? [name];

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
  // takes is passed as it is: `compile_streaming(window::fetch(..))`.
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
  // A `WindowProxy` is a `Window`, as a script sees it: `frame.contentWindow`.
  if (name === "WindowProxy") return rustType({ ...t, idlType: "Window" }, at);
  if (NUMBERS[name]) return NUMBERS[name];
  if (STRINGS.has(name) || enums.has(name)) return at === "param" ? "&str" : "String";
  if (name === "EventListener" && at === "param") return "Box<dyn FnMut(&Event)>";
  // Any JS object: a Rust value of any type in, an opaque object out.
  if (name === "object") return at === "param" ? "&dyn core::any::Any" : "&'static JsObject";
  const dictionary = dictionaries.get(name);
  if (dictionary && at === "result") return dictionaryType(dictionary);
  if (dictionary && at === "param") return paramDictionary(dictionary);
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

/** A field of a dictionary a function takes: its Rust and JS names, and type. */
type Field = { rust: string; js: string; type: string; optional: boolean };

/** The fields of each dictionary a function takes, and whether they borrow. */
const paramDictionaries = new Map<string, { fields: Field[]; borrows: boolean }>();

/** A dictionary's members, its parents' first: `AddEventListenerOptions` has `capture`. */
function dictionaryMembers(d: Def): Member[] {
  const parent = d.inheritance ? dictionaries.get(d.inheritance) : undefined;
  return [...(parent ? dictionaryMembers(parent) : []), ...((d.members ?? []) as Member[])];
}

/**
 * A dictionary a function takes is a Rust struct, as ReScript's is a record
 * of optional fields (ADR 0102): an optional member is an `Option`, `None`
 * unless given, which JS reads as not given, and with none required it has
 * `Default`: `RequestInit { method: Some("POST"), ..Default::default() }`.
 * What it borrows lives for `'a`. A member of a type Rust can't take is
 * left out; one of a union is its string, if it can be one.
 */
function paramDictionary(d: Def): string | { skip: string } {
  if (usedDictionaries.has(d.name)) return { skip: `${d.name} as a result too` };
  let known = paramDictionaries.get(d.name);
  if (!known) {
    const fields: Field[] = [];
    for (const m of dictionaryMembers(d)) {
      const alts = alternatives(m.idlType!);
      const given = FIELD_TYPES[`${d.name}.${m.name}`];
      const chosen = given ?? (isUnion(m.idlType!) ? alts.find((a) => a === "&str") ?? alts[0] : alts[0]);
      if (!chosen || chosen.includes("dyn core::any::Any")) continue;
      // A borrow in a field lives as long as the struct's: `&'a str`.
      const type = chosen.replace(/^&(?!')/, "&'a ").replace(/^([A-Z]\w*)<'_>$/, "$1<'a>");
      fields.push({ rust: snake(m.name!), js: m.name!, type, optional: !m.required });
    }
    if (fields.length === 0) return { skip: d.name };
    known = { fields, borrows: fields.some((f) => f.type.includes("'a")) };
    paramDictionaries.set(d.name, known);
  }
  return `${typeName(d.name)}${known.borrows ? "<'_>" : ""}`;
}

/** A parameter type that's a dictionary: `RequestInit<'_>`. */
const isDictionary = (rust: string) => paramDictionaries.has([...paramDictionaries.keys()].find((n) => rust.replace(/<'_>$/, "") === typeName(n)) ?? "");

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
      // A union's dictionary is named after the argument, as ReScript's
      // `~options=?` is: `add_event_listener_with_options`, then `_with_bool`.
      const word = (alt: string) => (union && !isDictionary(alt) ? suffix(alt) : snakeWords(a.name));
      if (union) alts.sort((x, y) => Number(isDictionary(y)) - Number(isDictionary(x)));
      for (const alt of union ? alts : alts.slice(0, 1)) {
        forms.push({ name: `${base}_with_${[...lead, ...words, word(alt)].join("_and_")}`, params: [...params, `${snake(a.name)}: ${alt}`] });
      }
      words.push(word(alts[0]));
      params.push(`${snake(a.name)}: ${alts[0]}`);
    }
    return forms;
  };

  // The form ReScript gives the plain name, first: `fetch(url)`.
  const primary = (key: string, sig: { names: string[]; options: string[][] }) => {
    const wanted = PRIMARY[key];
    if (!wanted) return sig;
    const options = sig.options.map((o) => {
      const k = o.findIndex((alt) => suffix(alt) === wanted);
      return k > 0 ? [o[k], ...o.filter((_, j) => j !== k)] : o;
    });
    return { names: sig.names, options };
  };

  const variants = (base: string, sig: { names: string[]; options: string[][] }, named = false) => {
    const varying = sig.options.findIndex((o) => o.length > 1);
    const pick = (k: number) => sig.options.map((o, j) => (j === varying ? o[k] : o[0]));
    const count = varying < 0 ? 1 : sig.options[varying].length;
    return Array.from({ length: count }, (_, k) => ({
      // `named`: each after its type, the first too, as ReScript's `from*`.
      name: named ? `${base}_${suffix(sig.options[varying][k])}` : k === 0 ? base : `${base}_with_${suffix(sig.options[varying][k])}`,
      params: pick(k).map((ty, j) => `${sig.names[j]}: ${ty}`),
    }));
  };

  for (const [index, { member: m }] of i.members.entries()) {
    if (m.type === "constructor") {
      if (!i.constructible) continue;
      const found = signatures(m.arguments ?? []);
      if ("skip" in found) {
        skip(found.skip);
        continue;
      }
      const sig = primary(`${i.name}.constructor`, found);
      // As ReScript names them: one signature is `new`, its optional
      // arguments `new_with_<name>`; a family of sources is `from_<type>`,
      // `request::from_url`, and the optional arguments of its first follow.
      const family = sig.options.some((o) => o.length > 1);
      const forms = family
        ? [...variants("from", sig, true), ...optionalForms(`from_${suffix(sig.options[sig.options.findIndex((o) => o.length > 1)][0])}`, [], sig, m.arguments ?? [])]
        : [...variants("new", sig), ...optionalForms("new", [], sig, m.arguments ?? [])];
      for (const v of forms) {
        fns.push({ name: v.name, jsName: `new ${jsName(i)}`, params: v.params, result: `&'static ${typeName(i.name)}`, doc: [`[MDN](${mdn(i.name, i.name)})`] });
      }
    } else if (m.type === "attribute") {
      if (m.special === "static" || i.isNamespace) {
        skip(m.special || "namespace attribute");
        continue;
      }
      const result = rustType(m.idlType!, "result");
      const doc = [`[MDN](${mdn(i.name, m.name)})`];
      // A getter whose type isn't supported (a union, say) is skipped, but
      // its setter can still take the union's first supported member.
      if (typeof result === "string") {
        fns.push({ name: snake(m.name!), jsName: `get ${m.name}`, params: self, result: orNull(result, m.idlType!), doc });
      } else {
        skip(result.skip);
      }
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
      const found = signatures(m.arguments ?? []);
      if (typeof result !== "string" || "skip" in found) {
        skip(typeof result !== "string" ? result.skip : (found as { skip: string }).skip);
        continue;
      }
      const sig = primary(`${i.name}.${m.name}`, found);
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

line(`//! The web platform for rust-js: DOM bindings generated by \`webapi/generate.ts\``);
line(`//! from W3C's WebIDL (\`@webref/idl\` ${webref.version}; specs: ${SPECS.join(", ")}). Do not edit.`);
line(`//!`);
line(`//! Each interface is a type (\`Element\`) and a module of its members`);
line(`//! (\`element::append\`). Inheritance is \`Deref\`, so an \`&HtmlButtonElement\``);
line(`//! goes wherever an \`&Element\` or \`&Node\` is expected. See ADR 0024.`);
line(`//! The JS language's own types, \`Promise\` and \`ArrayBuffer\` say, are the js crate's (ADR 0102).`);
line();
line(`// Many Rust functions call the same JS name: an overload per union member
// (\`before\`, \`before_with_str\`), and methods of the same name on different
// interfaces. rustc warns because in native code they would be one symbol.`);
line(`#![allow(clashing_extern_declarations)]`);
line();
line(`use core::marker::PhantomData;`);
line(`use core::ops::Deref;`);
line(`use js::{ArrayBuffer, JsObject, Promise, Uint8Array};`);
line();
line(`unsafe extern "Rust" {`);
GLOBALS.forEach(([name, type], k) => {
  if (k > 0) line();
  line(`    /// The \`${name}\` global.`);
  line(`    pub safe static ${name}: &'static ${type};`);
});
line(`}`);

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
    line(`        pub safe fn ${RENAMES[`${name}::${f.name}`] ?? f.name}(${f.params.join(", ")})${result};`);
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
  module(snake(qualified(name)), [...functionsOf(i), ...(EXTRA[name] ?? [])]);
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

// The dictionaries functions take (ADR 0102): records of optional fields.
for (const [name, { fields, borrows }] of [...paramDictionaries].sort(([a], [b]) => a.localeCompare(b))) {
  line();
  line(`/// The [\`${name}\`](https://developer.mozilla.org/docs/Web/API/${name}) dictionary: a JS object of these fields, a \`None\` one not given.`);
  if (fields.every((f) => f.optional)) line(`#[derive(Default)]`);
  line(`pub struct ${typeName(name)}${borrows ? "<'a>" : ""} {`);
  for (const f of fields) {
    if (f.rust !== f.js) line(`    #[cfg_attr(rust_js, rust_js::name = ${JSON.stringify(f.js)})]`);
    line(`    pub ${f.rust}: ${f.optional ? `Option<${f.type}>` : f.type},`);
  }
  line(`}`);
}

await Bun.write(new URL("./src/lib.rs", import.meta.url), `${out.join("\n")}\n`);
const reasons = [...skipped].sort((a, b) => b[1] - a[1]).map(([why, n]) => `${why} ${n}`);
console.log(`src/lib.rs: ${INTERFACES.length} interfaces, ${NAMESPACES.length} namespaces, ${count} functions`);
console.log(`skipped: ${reasons.slice(0, 12).join(", ")}${reasons.length > 12 ? ", ..." : ""}`);
