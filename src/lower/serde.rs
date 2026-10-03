//! serde's derives, and serde_json, as JS (ADR 0077).
//!
//! `#[derive(Serialize)]` writes an impl that drives any `Serializer`, with
//! generic machinery JS has no use for. rust-js leaves that impl's code out
//! and writes the steps it would take against one serializer, `$json`, which
//! lays JSON out as serde_json does:
//!
//! ```text
//!   #[derive(Serialize)] struct Order { id: u32, note: Option<String> }
//!
//!   function orderSerialize_serialize(order, json) {
//!     json.beginObject();
//!     json.key("id");
//!     json.int(order.id);
//!     json.key("note");
//!     if (order.note == null) json.null(); else json.string(order.note);
//!     json.endObject();
//!   }
//! ```
//!
//! What a type's JSON looks like is serde's to say, from its `#[serde(..)]`
//! attributes, read here as serde_derive reads them. Reading JSON back is
//! in `de`.

mod de;
mod value;

pub(super) use super::recognition::Json;

use super::bindings::variant_name;
use super::recognition::SkipPredicate;
use super::representation::{Num, variant_field};
use super::{FnCx, R, lower_first};
use crate::js::{self, Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_ast::visit::{self, Visitor};
use rustc_hir::def::CtorKind;
use rustc_hir::{self as hir, intravisit};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, sym};
use std::collections::HashMap;

/// One `#[serde(..)]` item: `rename = "a"`, or `rename(serialize = "a")`.
pub struct SerdeItem {
    name: String,
    value: Option<String>,
    nested: Vec<(String, Option<String>)>,
    span: Span,
}

/// Each item's, variant's and field's `#[serde(..)]`, by the span of its
/// name. HIR has no `#[serde]`: it's a derive's helper, so it's read from
/// the crate as it was expanded.
pub type SerdeAttributes = HashMap<Span, Vec<SerdeItem>>;

/// `#[serde(..)]` everywhere in the expanded crate, before rustc lowers it.
pub fn attributes(tcx: TyCtxt<'_>) -> SerdeAttributes {
    struct Collector(SerdeAttributes);
    impl Collector {
        fn record(&mut self, span: Span, attrs: &[rustc_ast::Attribute]) {
            for attr in attrs.iter().filter(|a| a.has_name(Symbol::intern("serde"))) {
                for item in attr.meta_item_list().unwrap_or_default() {
                    let nested = item
                        .meta_item_list()
                        .unwrap_or_default()
                        .iter()
                        .map(|n| {
                            (
                                n.name().map(|s| s.to_string()).unwrap_or_default(),
                                n.value_str().map(|v| v.to_string()),
                            )
                        })
                        .collect();
                    self.0.entry(span).or_default().push(SerdeItem {
                        name: item.name().map(|n| n.to_string()).unwrap_or_default(),
                        value: item.value_str().map(|v| v.to_string()),
                        nested,
                        span: item.span(),
                    });
                }
            }
        }
    }
    impl<'a> Visitor<'a> for Collector {
        fn visit_item(&mut self, item: &'a rustc_ast::Item) {
            if let Some(ident) = item.kind.ident() {
                self.record(ident.span, &item.attrs);
            }
            visit::walk_item(self, item);
        }
        fn visit_variant(&mut self, variant: &'a rustc_ast::Variant) {
            self.record(variant.ident.span, &variant.attrs);
            visit::walk_variant(self, variant);
        }
        fn visit_field_def(&mut self, field: &'a rustc_ast::FieldDef) {
            let span = field.ident.map_or(field.span, |ident| ident.span);
            self.record(span, &field.attrs);
            visit::walk_field_def(self, field);
        }
    }
    let krate = tcx.resolver_for_lowering().1.borrow();
    let mut collector = Collector(HashMap::new());
    visit::walk_crate(&mut collector, &krate);
    collector.0
}

/// `Some(true)` for serde's `Serialize`, `Some(false)` for `Deserialize`
/// and `DeserializeOwned`.
pub(super) use super::recognition::serde_trait;

/// How `rename_all` turns a Rust name into a JSON one: serde_derive's rules.
#[derive(Clone, Copy, PartialEq)]
enum Rule {
    Lower,
    Upper,
    Pascal,
    Camel,
    Snake,
    ScreamingSnake,
    Kebab,
    ScreamingKebab,
}

impl Rule {
    fn parse(name: &str) -> Option<Rule> {
        Some(match name {
            "lowercase" => Rule::Lower,
            "UPPERCASE" => Rule::Upper,
            "PascalCase" => Rule::Pascal,
            "camelCase" => Rule::Camel,
            "snake_case" => Rule::Snake,
            "SCREAMING_SNAKE_CASE" => Rule::ScreamingSnake,
            "kebab-case" => Rule::Kebab,
            "SCREAMING-KEBAB-CASE" => Rule::ScreamingKebab,
            _ => return None,
        })
    }

    /// A variant's name, written in `PascalCase`.
    fn variant(self, variant: &str) -> String {
        match self {
            Rule::Pascal => variant.to_owned(),
            Rule::Lower => variant.to_ascii_lowercase(),
            Rule::Upper => variant.to_ascii_uppercase(),
            Rule::Camel => variant[..1].to_ascii_lowercase() + &variant[1..],
            Rule::Snake => {
                let mut snake = String::new();
                for (i, ch) in variant.char_indices() {
                    if i > 0 && ch.is_uppercase() {
                        snake.push('_');
                    }
                    snake.push(ch.to_ascii_lowercase());
                }
                snake
            }
            Rule::ScreamingSnake => Rule::Snake.variant(variant).to_ascii_uppercase(),
            Rule::Kebab => Rule::Snake.variant(variant).replace('_', "-"),
            Rule::ScreamingKebab => Rule::ScreamingSnake.variant(variant).replace('_', "-"),
        }
    }

    /// A field's name, written in `snake_case`.
    fn field(self, field: &str) -> String {
        match self {
            Rule::Lower | Rule::Snake => field.to_owned(),
            Rule::Upper | Rule::ScreamingSnake => field.to_ascii_uppercase(),
            Rule::Pascal => {
                let mut pascal = String::new();
                let mut capitalize = true;
                for ch in field.chars() {
                    if ch == '_' {
                        capitalize = true;
                    } else if capitalize {
                        pascal.push(ch.to_ascii_uppercase());
                        capitalize = false;
                    } else {
                        pascal.push(ch);
                    }
                }
                pascal
            }
            Rule::Camel => {
                let pascal = Rule::Pascal.field(field);
                pascal[..1].to_ascii_lowercase() + &pascal[1..]
            }
            Rule::Kebab => field.replace('_', "-"),
            Rule::ScreamingKebab => field.to_ascii_uppercase().replace('_', "-"),
        }
    }
}

/// How an enum is written (serde's "enum representations").
#[derive(Clone, PartialEq)]
enum Tagging {
    /// `{"Circle": 2.5}`, the default.
    External,
    /// `#[serde(tag = "type")]`: `{"type": "Circle", ..}`.
    Internal(String),
    /// `#[serde(tag = "t", content = "c")]`: `{"t": "Circle", "c": 2.5}`.
    Adjacent(String, String),
    /// `#[serde(untagged)]`: `2.5`.
    Untagged,
}

/// `#[serde(default)]`, or `#[serde(default = "path")]`.
#[derive(Clone, Copy)]
enum SerdeDefault {
    Default,
    Path,
}

/// The `#[serde(..)]` attributes of a container, a variant or a field.
/// `rename` is the name JSON is written with, and `de_rename` the one it's
/// read with: `rename(serialize = "a", deserialize = "b")` tells them apart.
#[derive(Default)]
struct Attrs {
    rename: Option<String>,
    rename_all: Option<Rule>,
    rename_all_fields: Option<Rule>,
    de_rename: Option<String>,
    de_rename_all: Option<Rule>,
    de_rename_all_fields: Option<Rule>,
    aliases: Vec<String>,
    tag: Option<String>,
    content: Option<String>,
    untagged: bool,
    transparent: bool,
    skip_serializing: bool,
    skip_serializing_if: Option<String>,
    skip_deserializing: bool,
    default: Option<SerdeDefault>,
    deny_unknown_fields: bool,
    other: bool,
    expecting: Option<String>,
    /// `#[serde(from = "T")]`, `try_from` and `into`: the type is the one
    /// rustc resolved in the derive (`conversion`).
    from: bool,
    try_from: bool,
    into: bool,
    flatten: bool,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// `#[serde(..)]` on `def_id`, as serde_derive reads it.
    fn serde_attrs(&self, def_id: DefId) -> R<Attrs> {
        let mut attrs = Attrs::default();
        let span = self
            .tcx
            .def_ident_span(def_id)
            .unwrap_or_else(|| self.tcx.def_span(def_id));
        for item in self.krate.serde_attrs.get(&span).into_iter().flatten() {
            let name = item.name.as_str();
            let value = item.value.clone();
            // `rename(serialize = "a", deserialize = "b")`: the one JSON is
            // written with, and the one it's read with.
            let named = |which: &str| {
                value.clone().or_else(|| {
                    item.nested
                        .iter()
                        .find(|(n, _)| n == which)
                        .and_then(|(_, v)| v.clone())
                })
            };
            let (written, read) = (named("serialize"), named("deserialize"));
            let rule = |name: &Option<String>| -> R<Option<Rule>> {
                match name {
                    Some(name) => Rule::parse(name)
                        .map(Some)
                        .ok_or_else(|| self.unsupported(item.span, &format!("`rename_all = {name:?}`"))),
                    None => Ok(None),
                }
            };
            match name {
                "rename" => (attrs.rename, attrs.de_rename) = (written, read),
                "rename_all" => (attrs.rename_all, attrs.de_rename_all) = (rule(&written)?, rule(&read)?),
                "rename_all_fields" => {
                    (attrs.rename_all_fields, attrs.de_rename_all_fields) = (rule(&written)?, rule(&read)?)
                }
                "alias" => attrs.aliases.extend(value),
                "tag" => attrs.tag = value,
                "content" => attrs.content = value,
                "untagged" => attrs.untagged = true,
                "transparent" => attrs.transparent = true,
                "skip" => (attrs.skip_serializing, attrs.skip_deserializing) = (true, true),
                "skip_serializing" => attrs.skip_serializing = true,
                "skip_serializing_if" => attrs.skip_serializing_if = value,
                "skip_deserializing" => attrs.skip_deserializing = true,
                "default" => {
                    attrs.default = Some(if value.is_some() {
                        SerdeDefault::Path
                    } else {
                        SerdeDefault::Default
                    })
                }
                "deny_unknown_fields" => attrs.deny_unknown_fields = true,
                "other" => attrs.other = true,
                "expecting" => attrs.expecting = value,
                "from" => attrs.from = true,
                "try_from" => attrs.try_from = true,
                "into" => attrs.into = true,
                "flatten" => attrs.flatten = true,
                // Lifetimes and bounds, which rustc has checked; JS has neither.
                "borrow" | "bound" => {}
                _ => return Err(self.unsupported(item.span, &format!("`#[serde({name})]`"))),
            }
        }
        Ok(attrs)
    }

    /// The function that writes a `ty` value as JSON, the derived
    /// `Serialize::serialize` of the crate's own type, if it has one.
    fn serialize_fn(&self, ty: Ty<'tcx>) -> Option<DefId> {
        self.codec_fn(ty, true)
    }

    /// A type's derived `serialize`, or `deserialize`: the crate's own, or one
    /// a library of it exports (ADR 0100).
    pub(super) fn codec_fn(&self, ty: Ty<'tcx>, serialize: bool) -> Option<DefId> {
        let ty::Adt(adt, _) = ty.kind() else {
            return None;
        };
        let foreign = self.krate.foreign.in_library(adt.did());
        let impls = match foreign {
            true => self.tcx.trait_impls_in_crate(adt.did().krate),
            false => self.krate.trait_impls,
        };
        impls.iter().copied().find_map(|imp| {
            let tr = self.tcx.impl_trait_ref(imp).instantiate_identity().skip_normalization();
            let same = matches!(tr.self_ty().kind(), ty::Adt(a, _) if a.did() == adt.did());
            if !same || serde_trait(self.tcx, tr.def_id) != Some(serialize) {
                return None;
            }
            let method = self.tcx.associated_item_def_ids(imp)[0];
            (!foreign || self.krate.foreign.item(method).is_some()).then_some(method)
        })
    }

    /// A derived `serialize` or `deserialize`.
    pub(super) fn lower_codec(&mut self, method: DefId) -> R<js::Function> {
        match super::analysis::serde_impl(self.tcx, self.tcx.parent(method)) {
            Some(true) => self.lower_serialize(method),
            _ => self.lower_deserialize(method),
        }
    }

    /// `function orderSerialize_serialize(order, json) { .. }`: the derived
    /// `serialize`, as the steps it takes with `$json`.
    fn lower_serialize(&mut self, method: DefId) -> R<js::Function> {
        let imp = self.tcx.parent(method);
        let span = self.tcx.def_span(imp);
        let self_ty = self.tcx.type_of(imp).instantiate_identity().skip_normalization();
        let ty::Adt(adt, args) = *self_ty.kind() else {
            return Err(self.unsupported(span, "serializing this"));
        };
        let value = self.fresh(&lower_first(self.tcx.item_name(adt.did()).as_str()));
        let json = self.fresh("json");
        let mut params = vec![value.clone().into(), json.clone().into()];
        params.extend(self.codec_params(args, "write"));
        let mut body = Vec::new();
        self.write_adt(Expr::var(&value), &json, adt, args, span, &mut body)?;
        let js_span = self.js_span(span);
        Ok(js::Function {
            name: self.krate.fns[&method].name.clone(),
            params,
            body,
            export: false,
            is_async: false,
            span: js_span,
            name_span: js_span,
        })
    }

    /// `serde_json::to_string(&v)` and `to_string_pretty`: `$toJson((json) =>
    /// { .. }, pretty)`, the steps for `v` in a function the writer calls.
    pub(super) fn json_text(&mut self, value: Expr, ty: Ty<'tcx>, pretty: bool, span: Span) -> R<Expr> {
        self.runtime.insert(Helper::ToJson);
        let write = self.json_writer(ty, span)?;
        Ok(Expr::call(Expr::var("$toJson"), vec![value, write, Expr::bool(pretty)]))
    }

    /// The writer (`serialize`) or reader of a type parameter: a generic
    /// codec's parameter, or a generic function's evidence for its bound,
    /// `T: Serialize` or `T: DeserializeOwned` (ADR 0081).
    fn serde_evidence(&self, ty: Ty<'tcx>, serialize: bool) -> Option<Expr> {
        if let Some((_, name)) = self.codecs.iter().find(|(t, _)| *t == ty) {
            return Some(Expr::var(name));
        }
        self.given_evidence(|tr| tr.self_ty() == ty && serde_trait(self.tcx, tr.def_id) == Some(serialize))
    }

    /// A generic type's codec takes a function for each of its type
    /// parameters, `writeT` or `readT` (ADR 0080): their names, which the
    /// codec's type parameters now stand for.
    fn codec_params(&mut self, args: ty::GenericArgsRef<'tcx>, prefix: &str) -> Vec<js::Pattern> {
        self.codecs = Vec::new();
        let mut params = Vec::new();
        for t in args.types() {
            if let ty::Param(p) = t.kind() {
                let name = self.fresh(&format!("{prefix}{}", p.name));
                self.codecs.push((t, name.clone()));
                params.push(name.into());
            }
        }
        params
    }

    /// The function that writes a `ty` value, `(value, json) => ..`: a
    /// generic codec's parameter, the crate's own type's `serialize`, or
    /// the steps for it.
    pub(super) fn json_writer(&mut self, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let ty = ty.peel_refs();
        if let Some(writer) = self.serde_evidence(ty, true) {
            return Ok(writer);
        }
        if let Some(writer) = self.json_value_writer(ty) {
            return Ok(writer);
        }
        if let ty::Adt(_, args) = ty.kind()
            && args.types().next().is_none()
            && let Some(f) = self.serialize_fn(ty)
        {
            return Ok(self.fn_ref(f));
        }
        let (item, json) = (self.fresh("value"), self.fresh("json"));
        let mut body = Vec::new();
        self.write_json(Expr::var(&item), &json, ty, span, &mut body)?;
        Ok(Expr::arrow(vec![item.into(), json.into()], body))
    }

    /// `json.method(args)` as a statement.
    fn emit(&self, json: &str, method: &str, args: Vec<Expr>, out: &mut Vec<Stmt>) {
        let call = Expr::call(Expr::member(Expr::var(json), method), args);
        out.push(StmtKind::Expr(call).at(js::Span::NONE));
    }

    /// The steps that write `value`, a `ty`, as serde would.
    fn write_json(&mut self, value: Expr, json: &str, ty: Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<()> {
        let ty = ty.peel_refs();
        // A type parameter: its writer's call.
        if let Some(writer) = self.serde_evidence(ty, true).or_else(|| self.json_value_writer(ty)) {
            let call = Expr::call(writer, vec![value, Expr::var(json)]);
            out.push(StmtKind::Expr(call).at(js::Span::NONE));
            return Ok(());
        }
        // Each kind by its own method, which a flattened field tells apart.
        if ty.is_unit() {
            self.emit(json, "null", Vec::new(), out);
            return Ok(());
        }
        if ty.is_bool() {
            self.emit(json, "bool", vec![value], out);
            return Ok(());
        }
        // serde_json writes an `f32` with its own shortest digits, and a
        // `Value` of it is its `f64` (ADR 0122).
        if Num::of(ty) == Some(Num::F32) {
            self.emit(json, "number", vec![value, Expr::bool(true)], out);
            return Ok(());
        }
        if Num::of(ty).is_some_and(|n| !n.float()) {
            self.emit(json, "int", vec![value], out);
            return Ok(());
        }
        if Num::of(ty) == Some(Num::F64) {
            self.emit(json, "number", vec![value], out);
            return Ok(());
        }
        if self.is_string_like(ty) {
            self.emit(json, if ty.is_char() { "char" } else { "string" }, vec![value], out);
            return Ok(());
        }
        // Read more than once below.
        let value = if value.reads_same() {
            value
        } else {
            self.spill("value", value, out)
        };
        if let Some(inner) = self.option_of(ty) {
            // Of a type parameter, a `Some` may be boxed (ADR 0051).
            let payload = if self.boxed_payload(inner) {
                self.runtime.insert(Helper::SomeValue);
                Expr::call(Expr::var("$someValue"), vec![value.clone()])
            } else {
                value.clone()
            };
            let mut some = Vec::new();
            self.write_json(payload, json, inner, span, &mut some)?;
            let mut none = Vec::new();
            self.emit(json, "null", Vec::new(), &mut none);
            let test = Expr::bin(Op::LooseEq, value, Expr::null());
            out.push(StmtKind::If(test, none, Some(some)).at(js::Span::NONE));
            return Ok(());
        }
        match ty.kind() {
            ty::Adt(_, args) if ty.is_box() || self.is_rc(ty) => {
                self.write_json(value, json, args.type_at(0), span, out)
            }
            // `{"Ok": ..}` or `{"Err": ..}`, as serde's impl writes one.
            ty::Adt(_, args) if self.is_std_adt(ty, Symbol::intern("Result")) => {
                let mut branches = Vec::new();
                for (i, name) in ["Ok", "Err"].into_iter().enumerate() {
                    let mut body = Vec::new();
                    self.emit(json, "beginObject", Vec::new(), &mut body);
                    self.emit(json, "key", vec![Expr::str(name)], &mut body);
                    self.write_json(
                        Expr::member(value.clone(), "_0"),
                        json,
                        args.type_at(i),
                        span,
                        &mut body,
                    )?;
                    self.emit(json, "endObject", Vec::new(), &mut body);
                    branches.push(body);
                }
                let err = branches.pop().expect("an `Err` branch");
                let ok = branches.pop().expect("an `Ok` branch");
                let test = Expr::bin(Op::Eq, Expr::member(value, "TAG"), Expr::str("Ok"));
                out.push(StmtKind::If(test, ok, Some(err)).at(js::Span::NONE));
                Ok(())
            }
            ty::Adt(_, args) if self.is_vec_like(ty) || (self.is_set(ty) && !self.is_sorted(ty)) => {
                self.write_items(value, json, "beginArray", args.type_at(0), span, out)
            }
            ty::Adt(_, args) if self.is_set(ty) => {
                let items = self.in_order_of(value, ty, span)?;
                self.write_items(items, json, "beginArray", args.type_at(0), span, out)
            }
            // serde writes an array as a tuple.
            ty::Array(item, _) => self.write_items(value, json, "beginTuple", *item, span, out),
            ty::Slice(item) => self.write_items(value, json, "beginArray", *item, span, out),
            ty::Tuple(tys) => {
                let tys: Vec<Ty<'tcx>> = tys.to_vec();
                self.emit(json, "beginTuple", Vec::new(), out);
                for (i, t) in tys.into_iter().enumerate() {
                    self.emit(json, "element", Vec::new(), out);
                    self.write_json(Expr::index(value.clone(), Expr::int(i as i128)), json, t, span, out)?;
                }
                self.emit(json, "endArray", Vec::new(), out);
                Ok(())
            }
            ty::Adt(_, args) if self.is_map(ty) => {
                let (key_ty, item_ty) = (args.type_at(0), args.type_at(1));
                let entries = self.in_order_of(value, ty, span)?;
                let (key, item) = (self.fresh("key"), self.fresh("item"));
                let mut body = Vec::new();
                let key_text = self.json_key(Expr::var(&key), key_ty, span)?;
                self.emit(json, "key", vec![key_text], &mut body);
                self.write_json(Expr::var(&item), json, item_ty, span, &mut body)?;
                self.emit(json, "beginObject", Vec::new(), out);
                out.push(
                    StmtKind::ForOf {
                        label: None,
                        pattern: js::Pattern::Array(vec![Some(key), Some(item)]),
                        mutable: false,
                        iterable: entries,
                        body,
                    }
                    .at(js::Span::NONE),
                );
                self.emit(json, "endObject", Vec::new(), out);
                Ok(())
            }
            ty::Adt(_, args) if let Some(serialize) = self.serialize_fn(ty) => {
                let callee = self.fn_ref(serialize);
                let mut call_args = vec![value, Expr::var(json)];
                for t in args.types() {
                    call_args.push(self.json_writer(t, span)?);
                }
                out.push(StmtKind::Expr(Expr::call(callee, call_args)).at(js::Span::NONE));
                Ok(())
            }
            _ => Err(self.unsupported(span, &format!("serializing `{ty}`"))),
        }
    }

    /// `[a, b]`: each item, in its turn, after `begin`, `beginArray` or
    /// `beginTuple`.
    fn write_items(
        &mut self,
        items: Expr,
        json: &str,
        begin: &str,
        item_ty: Ty<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let item = self.fresh("item");
        let mut body = Vec::new();
        self.emit(json, "element", Vec::new(), &mut body);
        self.write_json(Expr::var(&item), json, item_ty, span, &mut body)?;
        self.emit(json, begin, Vec::new(), out);
        out.push(
            StmtKind::ForOf {
                label: None,
                pattern: js::Pattern::Name(item),
                mutable: false,
                iterable: items,
                body,
            }
            .at(js::Span::NONE),
        );
        self.emit(json, "endArray", Vec::new(), out);
        Ok(())
    }

    /// A map's key, as serde_json writes one: a string, or a number or a
    /// `bool` in quotes.
    fn json_key(&mut self, key: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let ty = ty.peel_refs();
        if self.is_string_like(ty) {
            return Ok(key);
        }
        if ty.is_bool() || Num::of(ty).is_some_and(|n| !n.float()) {
            return Ok(Expr::call(Expr::var("String"), vec![key]));
        }
        Err(self.unsupported(span, &format!("a map key of `{ty}` in JSON")))
    }

    /// A struct or an enum of the crate's own, by its `#[serde]` attributes.
    fn write_adt(
        &mut self,
        value: Expr,
        json: &str,
        adt: ty::AdtDef<'tcx>,
        args: ty::GenericArgsRef<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let container = self.serde_attrs(adt.did())?;
        let ty = Ty::new_adt(self.tcx, adt, args);
        // `#[serde(into = "T")]`: a clone of it, as a `T`, written as one.
        if container.into {
            let Some(target) = self.conversion(adt.did(), sym::Into, true) else {
                return Err(self.unsupported(span, "this `#[serde(into)]`"));
            };
            let cloned = self.clone_value(value, ty, span, out)?;
            let converted = self.convert(sym::From, target, ty, cloned, span)?;
            return self.write_json(converted, json, target, span, out);
        }
        if adt.is_struct() {
            let variant = adt.non_enum_variant();
            let fields: Vec<(Expr, Ty<'tcx>)> = (0..variant.fields.len())
                .map(|i| {
                    (
                        self.project(value.clone(), ty, i),
                        variant
                            .fields
                            .iter()
                            .nth(i)
                            .map(|field| self.field_ty(field, args))
                            .expect("a field"),
                    )
                })
                .collect();
            // `#[serde(transparent)]` and a newtype `struct Id(u32)`: the field itself.
            if container.transparent || (variant.ctor_kind() == Some(CtorKind::Fn) && fields.len() == 1) {
                let written = self.written_field(variant, &fields)?;
                let (field, field_ty) = written
                    .into_iter()
                    .next()
                    .ok_or_else(|| self.unsupported(span, "this struct"))?;
                return self.write_json(field, json, field_ty, span, out);
            }
            return match variant.ctor_kind() {
                Some(CtorKind::Const) => {
                    self.emit(json, "null", Vec::new(), out);
                    Ok(())
                }
                Some(CtorKind::Fn) => {
                    self.emit(json, "beginTupleStruct", Vec::new(), out);
                    self.write_tuple_fields(json, variant, &fields, span, out)?;
                    self.emit(json, "endArray", Vec::new(), out);
                    Ok(())
                }
                None => {
                    self.emit(json, "beginObject", Vec::new(), out);
                    self.write_struct_tag(json, adt, &container, out);
                    self.write_fields(json, variant, &fields, container.rename_all, span, out)?;
                    self.emit(json, "endObject", Vec::new(), out);
                    Ok(())
                }
            };
        }
        let tagging = match (&container.tag, &container.content, container.untagged) {
            (_, _, true) => Tagging::Untagged,
            (Some(tag), Some(content), _) => Tagging::Adjacent(tag.clone(), content.clone()),
            (Some(tag), None, _) => Tagging::Internal(tag.clone()),
            _ => Tagging::External,
        };
        // Each variant in turn: `if (v === "Dot") .. else if (v.TAG === "Ring") ..`.
        let mut chain: Option<Vec<Stmt>> = None;
        for variant in adt.variants().iter().rev() {
            let attrs = self.serde_attrs(variant.def_id)?;
            let rust_name = variant.name.to_string();
            let name = attrs.rename.clone().unwrap_or_else(|| {
                container
                    .rename_all
                    .map_or(rust_name.clone(), |r| r.variant(&rust_name))
            });
            let mut body = Vec::new();
            if attrs.skip_serializing {
                self.runtime.insert(Helper::JsonFail);
                let message = format!(
                    "the enum variant {}::{} cannot be serialized",
                    self.tcx.item_name(adt.did()),
                    rust_name
                );
                body.push(
                    StmtKind::Throw(Expr::call(Expr::var("$jsonError"), vec![Expr::str(message)])).at(js::Span::NONE),
                );
            } else {
                let fields: Vec<(Expr, Ty<'tcx>)> = variant
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        (
                            Expr::member(value.clone(), variant_field(self.tcx, variant, i)),
                            self.field_ty(f, args),
                        )
                    })
                    .collect();
                let fields_rule = attrs.rename_all.or(container.rename_all_fields);
                let tagging = if attrs.untagged { &Tagging::Untagged } else { &tagging };
                self.write_variant(json, tagging, &name, variant, &fields, fields_rule, span, &mut body)?;
            }
            chain = Some(match chain {
                None => body,
                Some(rest) => {
                    let js_name = Expr::str(variant_name(self.tcx, variant));
                    let test = if variant.fields.is_empty() {
                        Expr::bin(Op::Eq, value.clone(), js_name)
                    } else {
                        Expr::bin(Op::Eq, Expr::member(value.clone(), "TAG"), js_name)
                    };
                    vec![StmtKind::If(test, body, Some(rest)).at(js::Span::NONE)]
                }
            });
        }
        out.extend(chain.unwrap_or_default());
        Ok(())
    }

    /// The fields that aren't skipped, as `(value, type)`: for a
    /// `#[serde(transparent)]` struct, the one that's written.
    fn written_field(&self, variant: &ty::VariantDef, fields: &[(Expr, Ty<'tcx>)]) -> R<Vec<(Expr, Ty<'tcx>)>> {
        let mut written = Vec::new();
        for (field, (value, ty)) in variant.fields.iter().zip(fields) {
            if !self.serde_attrs(field.did)?.skip_serializing {
                written.push((value.clone(), *ty));
            }
        }
        Ok(written)
    }

    fn write_struct_tag(&self, json: &str, adt: ty::AdtDef<'tcx>, attrs: &Attrs, out: &mut Vec<Stmt>) {
        if let Some(tag) = &attrs.tag {
            let name = attrs
                .rename
                .clone()
                .unwrap_or_else(|| self.tcx.item_name(adt.did()).to_string());
            self.emit(json, "key", vec![Expr::str(tag.as_str())], out);
            self.emit(json, "string", vec![Expr::str(name)], out);
        }
    }

    fn write_tuple_fields(
        &mut self,
        json: &str,
        variant: &ty::VariantDef,
        fields: &[(Expr, Ty<'tcx>)],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        for (field, (value, ty)) in variant.fields.iter().zip(fields) {
            let attrs = self.serde_attrs(field.did)?;
            if attrs.skip_serializing {
                continue;
            }
            let mut body = Vec::new();
            self.emit(json, "element", Vec::new(), &mut body);
            self.write_json(value.clone(), json, *ty, span, &mut body)?;
            if let Some(path) = attrs.skip_serializing_if {
                let skip = self.skip_test(field.did, &path, value.clone(), *ty, span)?;
                out.push(StmtKind::If(super::std_impls::negate(skip), body, None).at(js::Span::NONE));
            } else {
                out.extend(body);
            }
        }
        Ok(())
    }

    /// Merge a struct payload into an internally tagged object. Unwrap its
    /// serialization representation first, rather than exposing wrapper fields.
    fn write_internal_fields(
        &mut self,
        value: Expr,
        json: &str,
        ty: Ty<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let ty = ty.peel_refs();
        let ty::Adt(adt, args) = *ty.kind() else {
            return Err(self.unsupported(span, "an internally tagged newtype variant of this"));
        };
        if !adt.is_struct() {
            return Err(self.unsupported(span, "an internally tagged newtype variant of this"));
        }
        let variant = adt.non_enum_variant();
        let attrs = self.serde_attrs(adt.did())?;
        let fields: Vec<_> = variant
            .fields
            .iter()
            .enumerate()
            .map(|(i, field)| (self.project(value.clone(), ty, i), self.field_ty(field, args)))
            .collect();
        if attrs.transparent || (variant.ctor_kind() == Some(CtorKind::Fn) && fields.len() == 1) {
            let (value, ty) = self
                .written_field(variant, &fields)?
                .into_iter()
                .next()
                .ok_or_else(|| self.unsupported(span, "an internally tagged newtype variant of this"))?;
            return self.write_internal_fields(value, json, ty, span, out);
        }
        match variant.ctor_kind() {
            None => {
                self.write_struct_tag(json, adt, &attrs, out);
                self.write_fields(json, variant, &fields, attrs.rename_all, span, out)
            }
            Some(CtorKind::Const) => Ok(()),
            Some(CtorKind::Fn) => Err(self.unsupported(span, "an internally tagged tuple struct")),
        }
    }

    /// A struct's (or a struct variant's) fields, inside its `{ .. }`.
    fn write_fields(
        &mut self,
        json: &str,
        variant: &ty::VariantDef,
        fields: &[(Expr, Ty<'tcx>)],
        rule: Option<Rule>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        for (field, (value, ty)) in variant.fields.iter().zip(fields) {
            let attrs = self.serde_attrs(field.did)?;
            if attrs.skip_serializing {
                continue;
            }
            let rust_name = field.name.to_string();
            let rust_name = rust_name.strip_prefix("r#").unwrap_or(&rust_name).to_string();
            let name = attrs
                .rename
                .unwrap_or_else(|| rule.map_or(rust_name.clone(), |r| r.field(&rust_name)));
            let mut body = Vec::new();
            // `#[serde(flatten)]`: its entries, among these.
            if attrs.flatten {
                let writer = self.json_writer(*ty, span)?;
                self.emit(json, "flat", vec![value.clone(), writer], &mut body);
            } else {
                self.emit(json, "key", vec![Expr::str(name)], &mut body);
                // Written only when it's `Some`, so it's written as what it holds.
                let written = match (attrs.skip_serializing_if.as_ref(), self.option_of(*ty)) {
                    (Some(_), Some(inner))
                        if !self.boxed_payload(inner)
                            && self
                                .resolved_skip(field.did)
                                .is_some_and(|f| self.recognition().skips_none(f)) =>
                    {
                        inner
                    }
                    _ => *ty,
                };
                self.write_json(value.clone(), json, written, span, &mut body)?;
            }
            match attrs.skip_serializing_if {
                Some(path) => {
                    let skip = self.skip_test(field.did, &path, value.clone(), *ty, span)?;
                    out.push(StmtKind::If(super::std_impls::negate(skip), body, None).at(js::Span::NONE));
                }
                None => out.extend(body),
            }
        }
        Ok(())
    }

    /// `#[serde(skip_serializing_if = "Option::is_none")]`: the test, for
    /// std's own. The crate's own function is called as it's named.
    fn skip_test(&mut self, field: DefId, path: &str, value: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let Some(f) = self.resolved_skip(field) else {
            return Err(self.unsupported(span, &format!("`skip_serializing_if = {path:?}` of `{ty}`")));
        };
        match self.recognition().skip_predicate(f, ty) {
            Some(SkipPredicate::None) => Ok(Expr::bin(Op::LooseEq, value, Expr::null())),
            Some(SkipPredicate::Some) => Ok(Expr::bin(Op::LooseNe, value, Expr::null())),
            Some(SkipPredicate::Empty) => Ok(Expr::bin(Op::Eq, Expr::member(value, "length"), Expr::int(0))),
            _ if self.krate.fns.contains_key(&f) => Ok(Expr::call(self.fn_ref(f), vec![value])),
            _ => Err(self.unsupported(span, &format!("`skip_serializing_if = {path:?}` of `{ty}`"))),
        }
    }

    /// `<to as From<from>>::from(value)` (or `TryFrom`), of the crate's own impl.
    fn convert(&mut self, convert: Symbol, to: Ty<'tcx>, from: Ty<'tcx>, value: Expr, span: Span) -> R<Expr> {
        let trait_id = self.tcx.get_diagnostic_item(convert).expect("std has it");
        let method = self
            .tcx
            .associated_items(trait_id)
            .in_definition_order()
            .find(|item| item.is_fn())
            .expect("a conversion method")
            .def_id;
        let args = self.tcx.mk_args(&[to.into(), from.into()]);
        self.impl_call(method, args, vec![value], span)
    }

    /// Serde's derive has already asked rustc to resolve the predicate. Its
    /// path retains the attribute's source span, including imports and aliases.
    fn resolved_skip(&self, field: DefId) -> Option<DefId> {
        self.resolved_path(field, "skip_serializing_if", true)
    }

    /// The function a `#[serde(name = "path")]` on `def_id` names, as rustc
    /// resolved it in the derived `Serialize` impl, or `Deserialize`'s.
    fn resolved_path(&self, def_id: DefId, name: &str, serialize: bool) -> Option<DefId> {
        struct Predicate<'tcx> {
            types: &'tcx ty::TypeckResults<'tcx>,
            span: Span,
            found: Option<DefId>,
        }
        impl<'tcx> intravisit::Visitor<'tcx> for Predicate<'tcx> {
            fn visit_expr(&mut self, expr: &'tcx hir::Expr<'tcx>) {
                if let hir::ExprKind::Path(ref path) = expr.kind
                    && self.span.lo() <= expr.span.lo()
                    && expr.span.hi() <= self.span.hi()
                    && let hir::def::Res::Def(_, id) = self.types.qpath_res(path, expr.hir_id)
                {
                    self.found = Some(id);
                }
                intravisit::walk_expr(self, expr);
            }
        }
        let item_span = self
            .tcx
            .def_ident_span(def_id)
            .unwrap_or_else(|| self.tcx.def_span(def_id));
        let span = self
            .krate
            .serde_attrs
            .get(&item_span)?
            .iter()
            .find(|item| item.name == name)?
            .span;
        for owner in self.tcx.hir_body_owners() {
            // Deserializing, it's called in the derive's visitor, inside the impl.
            let mut parent = self.tcx.opt_parent(owner.to_def_id());
            while let Some(id) = parent
                && super::analysis::serde_impl(self.tcx, id).is_none()
            {
                parent = self.tcx.opt_parent(id);
            }
            if parent.is_none_or(|parent| super::analysis::serde_impl(self.tcx, parent) != Some(serialize)) {
                continue;
            }
            let body = self.tcx.hir_body_owned_by(owner);
            let mut predicate = Predicate {
                types: self.tcx.typeck_body(body.id()),
                span,
                found: None,
            };
            intravisit::Visitor::visit_body(&mut predicate, body);
            if predicate.found.is_some() {
                return predicate.found;
            }
        }
        None
    }

    /// One enum variant, as the enum's tagging writes it.
    #[allow(clippy::too_many_arguments)]
    fn write_variant(
        &mut self,
        json: &str,
        tagging: &Tagging,
        name: &str,
        variant: &ty::VariantDef,
        fields: &[(Expr, Ty<'tcx>)],
        fields_rule: Option<Rule>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let kind = variant.ctor_kind();
        // The variant's content, without its tag.
        let content = |this: &mut Self, out: &mut Vec<Stmt>| -> R<()> {
            match kind {
                Some(CtorKind::Const) => {
                    this.emit(json, "null", Vec::new(), out);
                    Ok(())
                }
                Some(CtorKind::Fn) if fields.len() == 1 => {
                    this.write_json(fields[0].0.clone(), json, fields[0].1, span, out)
                }
                Some(CtorKind::Fn) => {
                    this.emit(json, "beginTuple", Vec::new(), out);
                    this.write_tuple_fields(json, variant, fields, span, out)?;
                    this.emit(json, "endArray", Vec::new(), out);
                    Ok(())
                }
                None => {
                    this.emit(json, "beginObject", Vec::new(), out);
                    this.write_fields(json, variant, fields, fields_rule, span, out)?;
                    this.emit(json, "endObject", Vec::new(), out);
                    Ok(())
                }
            }
        };
        match tagging {
            Tagging::External if kind == Some(CtorKind::Const) => {
                self.emit(json, "variant", vec![Expr::str(name)], out);
            }
            Tagging::External => {
                self.emit(json, "beginObject", Vec::new(), out);
                self.emit(json, "key", vec![Expr::str(name)], out);
                content(self, out)?;
                self.emit(json, "endObject", Vec::new(), out);
            }
            Tagging::Untagged => content(self, out)?,
            Tagging::Adjacent(tag, body) => {
                self.emit(json, "beginObject", Vec::new(), out);
                self.emit(json, "key", vec![Expr::str(tag.as_str())], out);
                self.emit(json, "string", vec![Expr::str(name)], out);
                if kind != Some(CtorKind::Const) {
                    self.emit(json, "key", vec![Expr::str(body.as_str())], out);
                    content(self, out)?;
                }
                self.emit(json, "endObject", Vec::new(), out);
            }
            Tagging::Internal(tag) => {
                self.emit(json, "beginObject", Vec::new(), out);
                self.emit(json, "key", vec![Expr::str(tag.as_str())], out);
                self.emit(json, "string", vec![Expr::str(name)], out);
                match kind {
                    Some(CtorKind::Const) => {}
                    None => self.write_fields(json, variant, fields, fields_rule, span, out)?,
                    // A newtype variant of a struct: the struct's fields, beside the tag.
                    Some(CtorKind::Fn) if fields.len() == 1 => {
                        let (inner, inner_ty) = fields[0].clone();
                        self.write_internal_fields(inner, json, inner_ty, span, out)?;
                    }
                    Some(CtorKind::Fn) => return Err(self.unsupported(span, "an internally tagged tuple variant")),
                }
                self.emit(json, "endObject", Vec::new(), out);
            }
        }
        Ok(())
    }
}
