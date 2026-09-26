//! Decode the binding language independently of call lowering.

use rustc_hir::def::DefKind;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::DefId;
use rustc_span::{Symbol, sym};

/// Validate tool bindings even if no function calls them. A malformed binding
/// must not silently become an ordinary Rust function with an unreachable body.
pub(super) fn validate(tcx: TyCtxt<'_>) -> bool {
    let mut valid = true;
    for def in tcx.hir_crate_items(()).definitions() {
        let attrs: Vec<_> = tcx
            .get_attrs_by_path(def.to_def_id(), &[Symbol::intern("rust_js"), sym::link_name])
            .collect();
        for attr in &attrs {
            if attrs.len() != 1
                || attr.value_str().is_none()
                || !matches!(tcx.def_kind(def), DefKind::Fn | DefKind::AssocFn)
            {
                tcx.dcx().span_err(
                    attr.span(),
                    "rust-js: a binding needs one `#[rust_js::link_name = \"...\"]` on a function or method",
                );
                valid = false;
            }
        }
    }
    valid
}

/// What a JS function or global declared in an `extern` block is called:
/// its `#[link_name]`, or its Rust name. A dotted name (`console.log`) is
/// a path from a global.
pub(super) fn js_name(tcx: TyCtxt<'_>, def_id: DefId) -> String {
    if let Some(name) = tool_link_name(tcx, def_id) {
        return name.to_string();
    }
    match tcx.codegen_fn_attrs(def_id).symbol_name {
        Some(name) => name.to_string(),
        None => tcx.item_name(def_id).to_string(),
    }
}

/// A binding written as an ordinary function, which can be generic, as an
/// `extern` one can't (ADR 0039): `#[rust_js::link_name = "react#useState"]`.
/// Its body is never compiled.
pub(super) fn tool_link_name(tcx: TyCtxt<'_>, def_id: DefId) -> Option<Symbol> {
    if !matches!(tcx.def_kind(def_id), DefKind::Fn | DefKind::AssocFn) {
        return None;
    }
    tcx.get_attrs_by_path(def_id, &[Symbol::intern("rust_js"), sym::link_name])
        .next()?
        .value_str()
}

/// Is this function or static JS's: in an `extern` block, or a
/// `#[rust_js::link_name]` function?
pub(super) fn is_binding(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    tcx.is_foreign_item(def_id) || tool_link_name(tcx, def_id).is_some()
}

/// A JS function whose first parameter is named `this` is a method:
/// `f(x, a)` calls `x.f(a)`. So is a Rust method's `self`.
pub(super) fn is_method(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    match tcx.def_kind(def_id) {
        DefKind::Fn => matches!(tcx.fn_arg_idents(def_id).first(), Some(Some(ident)) if ident.name.as_str() == "this"),
        DefKind::AssocFn => tcx.associated_item(def_id).is_method(),
        _ => false,
    }
}

/// How a call to a JS function is written, from its `#[link_name]` (ADR 0024).
pub(super) enum JsForm {
    /// `name(..)`, or `this.name(..)` for a method.
    Call(String),
    /// `this.name`.
    Get(String),
    /// `this.name = value`.
    Set(String),
    /// `new Name(..)`.
    New(String),
    /// `this` itself: an unchecked cast.
    This,
    /// `this(..)`: `this` is a JS function, like React's `setCount`.
    CallThis,
    /// A JSX element (ADR 0040): `<div>`, `<>`, an imported component like
    /// `<react#StrictMode>`, or `<*>` for the component given first.
    Jsx(String),
    /// `prop className`: a JSX attribute of `this`, the element being built.
    /// Just `prop`: the attribute's name comes first, as a string literal.
    Prop(Option<String>),
    /// `this instanceof Class`: a checked one, as a `bool`.
    InstanceOf(String),
}

pub(super) fn js_form(tcx: TyCtxt<'_>, def_id: DefId) -> JsForm {
    let name = js_name(tcx, def_id);
    match name.as_str() {
        "this" => return JsForm::This,
        "this()" => return JsForm::CallThis,
        "prop" => return JsForm::Prop(None),
        _ => {}
    }
    if let Some(tag) = name.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        return JsForm::Jsx(tag.to_string());
    }
    if let Some(prop) = name.strip_prefix("prop ") {
        return JsForm::Prop(Some(prop.to_string()));
    }
    let forms: [(&str, fn(String) -> JsForm); 4] = [
        ("get ", JsForm::Get),
        ("set ", JsForm::Set),
        ("new ", JsForm::New),
        ("instanceof ", JsForm::InstanceOf),
    ];
    for (prefix, form) in forms {
        if let Some(rest) = name.strip_prefix(prefix) {
            return form(rest.to_string());
        }
    }
    JsForm::Call(name)
}

/// The path a JS item is reached by, if it isn't a method or a property:
/// `document`, `console.log`, `Event` for `new Event`, or an import like
/// `node:path#join`.
pub(super) fn js_path(tcx: TyCtxt<'_>, def_id: DefId) -> Option<String> {
    match tcx.def_kind(def_id) {
        DefKind::Static { .. } => Some(js_name(tcx, def_id)),
        DefKind::Fn | DefKind::AssocFn => match js_form(tcx, def_id) {
            JsForm::Call(name) | JsForm::New(name) if !is_method(tcx, def_id) => Some(name),
            // A class to test against is a global or an import like any other.
            JsForm::InstanceOf(class) => Some(class),
            // So is a component: `<react#StrictMode>`.
            JsForm::Jsx(tag) if tag.contains('#') => Some(tag),
            _ => None,
        },
        _ => None,
    }
}

/// What a JS module exports under a name, `("node:path", "join")`, or
/// `"default"` or `"*"` for its default export or the module itself.
pub(super) type Export = (String, String);

/// An import from a JS module (ADR 0028): `"@codemirror/state#EditorState.create"`
/// is the export `("@codemirror/state", "EditorState")`, then the rest of
/// the path, `".create"`. The last `#` splits them, since a module's name
/// can start with one (Node's `#internal`).
pub(super) fn js_import(path: &str) -> Option<(Export, &str)> {
    let (from, path) = path.rsplit_once('#')?;
    let export = path.split('.').next().unwrap_or_default();
    (!from.is_empty() && !export.is_empty()).then(|| ((from.to_string(), export.to_string()), &path[export.len()..]))
}

/// What a default or namespace import is called: after its module, as in
/// ReScript. `./greet.js` is `greet`, and `@codemirror/lang-rust` is `langRust`.
pub(super) fn module_binding(from: &str) -> String {
    let file = from.rsplit(['/', ':']).find(|s| !s.is_empty()).unwrap_or_default();
    let stem = file.split('.').next().unwrap_or_default();
    let words = stem
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .filter(|w| !w.is_empty());
    let mut name = String::new();
    for (i, word) in words.enumerate() {
        let mut chars = word.chars();
        if i > 0
            && let Some(first) = chars.next()
        {
            name.push(first.to_ascii_uppercase());
        }
        name.extend(chars);
    }
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    }
}
