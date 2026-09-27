//! `#[derive(Deserialize)]` and `serde_json::from_str`, as JS (ADR 0078).
//!
//! serde's derive writes a visitor, which serde_json's `Deserializer` drives
//! through the text. Of that visitor, rust-js writes what it knows about the
//! type, each field's name and how to read it, as a table, and the runtime's
//! `$JsonReader`, serde_json's own steps ported, reads by it:
//!
//! ```text
//!   #[derive(Deserialize)] struct Order { id: u32, note: Option<String> }
//!
//!   function orderDeserialize_deserialize(json) {
//!     return json.struct(
//!       "struct Order",
//!       [["id", $json.u32], ["note", $json.option($json.string)]],
//!       ([id, note]) => ({ id, note }),
//!     );
//!   }
//! ```

use super::{Attrs, Rule, SerdeDefault, serde_trait};
use crate::js::{self, Expr, Op, Pattern, Prop, Stmt, StmtKind};
use crate::lower::bindings::variant_name;
use crate::lower::representation::Num;
use crate::lower::{FnCx, R, Shape};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_hir::def::CtorKind;
use rustc_middle::ty::{self, Ty};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol};

/// A struct's or a variant's fields, as `json.struct` and `json.tupleStruct`
/// take them.
struct Table {
    /// `[name, read]` or `[name, read, missing]`; for a tuple, `read` or
    /// `[read, missing]`.
    entries: Vec<Expr>,
    /// `([a, b], defaults) => ({ a, b, c: 0 })`, or `None` when it would
    /// give back the values as they are.
    build: Option<Expr>,
    /// `{ deny: true, container: () => .., expecting: ".." }`.
    options: Vec<Prop>,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// The function that reads a `ty` value from JSON, the derived
    /// `Deserialize::deserialize` of the crate's own type, if it has one.
    fn deserialize_fn(&self, ty: Ty<'tcx>) -> Option<DefId> {
        let ty::Adt(adt, _) = ty.kind() else {
            return None;
        };
        self.krate.trait_impls.iter().copied().find_map(|imp| {
            let tr = self.tcx.impl_trait_ref(imp).instantiate_identity();
            let same = matches!(tr.self_ty().kind(), ty::Adt(a, _) if a.did() == adt.did());
            (same && serde_trait(self.tcx, tr.def_id) == Some(false)).then(|| self.tcx.associated_item_def_ids(imp)[0])
        })
    }

    fn use_reader(&mut self) {
        self.runtime
            .extend([Helper::FromJson, Helper::JsonFail, Helper::DebugStr]);
    }

    /// `serde_json::from_str::<T>(s)`: `$fromJson(s, read)`, with `read` the
    /// function that reads a `T`.
    pub(in crate::lower) fn json_value(&mut self, text: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let read = self.json_reader(ty, span)?;
        self.use_reader();
        Ok(Expr::call(Expr::var("$fromJson"), vec![text, read]))
    }

    /// `function orderDeserialize_deserialize(json) { .. }`: the derived
    /// `deserialize`, reading from serde_json's reader.
    pub(super) fn lower_deserialize(&mut self, method: DefId) -> R<js::Function> {
        let imp = self.tcx.parent(method);
        let span = self.tcx.def_span(imp);
        let self_ty = self.tcx.type_of(imp).instantiate_identity();
        let ty::Adt(adt, args) = *self_ty.kind() else {
            return Err(self.unsupported(span, "deserializing this"));
        };
        if self
            .tcx
            .generics_of(imp)
            .own_params
            .iter()
            .any(|p| !matches!(p.kind, ty::GenericParamDefKind::Lifetime))
        {
            return Err(self.unsupported(span, "`Deserialize` of a generic type"));
        }
        self.use_reader();
        let json = self.fresh("json");
        let value = self.read_adt(&json, adt, args, span)?;
        let js_span = self.js_span(span);
        Ok(js::Function {
            name: self.krate.fns[&method].name.clone(),
            params: vec![json.into()],
            body: vec![StmtKind::Return(Some(value)).at(js_span)],
            export: false,
            is_async: false,
            span: js_span,
            name_span: js_span,
        })
    }

    /// The function that reads a `ty` as serde's impl for it does:
    /// `$json.u32`, `$json.vec($json.string)`, `orderDeserialize_deserialize`.
    fn json_reader(&mut self, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let reader = |name: &str| Expr::member(Expr::var("$json"), name);
        let unsupported = |this: &Self| this.unsupported(span, &format!("deserializing `{ty}`"));
        if ty.is_unit() {
            return Ok(reader("unit"));
        }
        if ty.is_bool() {
            return Ok(reader("bool"));
        }
        // Named as serde names them in its messages: `u32`, `usize`.
        if Num::of(ty).is_some() {
            return Ok(reader(&ty.to_string()));
        }
        if ty.is_char() {
            return Ok(reader("char"));
        }
        if self.is_lang_adt(ty, LangItem::String) {
            return Ok(reader("string"));
        }
        if let Some(inner) = self.option_of(ty) {
            // `Some(None)` would be `None` (ADR 0030).
            if self.can_be_nullish(inner) {
                return Err(unsupported(self));
            }
            let read = self.json_reader(inner, span)?;
            return Ok(Expr::call(reader("option"), vec![read]));
        }
        match ty.kind() {
            ty::Adt(_, args) if ty.is_box() || self.is_std_adt(ty, Symbol::intern("Rc")) => {
                self.json_reader(args.type_at(0), span)
            }
            // A heap would have to be put in its order.
            ty::Adt(..) if self.is_std_adt(ty, Symbol::intern("BinaryHeap")) => Err(unsupported(self)),
            ty::Adt(_, args) if self.is_vec_like(ty) => {
                let read = self.json_reader(args.type_at(0), span)?;
                Ok(Expr::call(reader("vec"), vec![read]))
            }
            ty::Adt(_, args) if self.is_set(ty) => {
                let read = self.json_reader(args.type_at(0), span)?;
                Ok(Expr::call(reader("set"), vec![read]))
            }
            ty::Adt(_, args) if self.is_map(ty) => {
                let key = self.json_key_reader(args.type_at(0), span)?;
                let read = self.json_reader(args.type_at(1), span)?;
                Ok(Expr::call(reader("map"), vec![key, read]))
            }
            ty::Array(item, len) => {
                let Some(len) = len.try_to_target_usize(self.tcx) else {
                    return Err(unsupported(self));
                };
                let read = self.json_reader(*item, span)?;
                Ok(Expr::call(reader("array"), vec![Expr::int(len as i128), read]))
            }
            ty::Tuple(tys) => {
                let reads = tys.iter().map(|t| self.json_reader(t, span)).collect::<R<_>>()?;
                Ok(Expr::call(reader("tuple"), reads))
            }
            ty::Adt(..) if let Some(deserialize) = self.deserialize_fn(ty) => Ok(self.fn_ref(deserialize)),
            _ => Err(unsupported(self)),
        }
    }

    /// An object's key, from inside its quotes: a string, a number or a `bool`.
    fn json_key_reader(&mut self, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let key = |name: &str| Expr::member(Expr::member(Expr::var("$json"), "key"), name);
        if self.is_lang_adt(ty, LangItem::String) {
            return Ok(key("string"));
        }
        if ty.is_char() {
            return Ok(key("char"));
        }
        if ty.is_bool() {
            return Ok(key("bool"));
        }
        if Num::of(ty).is_some() {
            let read = self.json_reader(ty, span)?;
            return Ok(Expr::call(key("number"), vec![read]));
        }
        Err(self.unsupported(span, &format!("a map key of `{ty}` in JSON")))
    }

    /// A struct or an enum of the crate's own, read from `json` as its
    /// derived `deserialize` reads it.
    fn read_adt(&mut self, json: &str, adt: ty::AdtDef<'tcx>, args: ty::GenericArgsRef<'tcx>, span: Span) -> R<Expr> {
        let container = self.serde_attrs(adt.did())?;
        let type_name = self.tcx.item_name(adt.did()).to_string();
        let method = |name: &str, args: Vec<Expr>| Expr::call(Expr::member(Expr::var(json), name), args);
        if adt.is_enum() {
            return self.read_enum(json, adt, args, &container, &type_name, span);
        }
        if container.tag.is_some() {
            return Err(self.unsupported(span, "deserializing a struct with `#[serde(tag)]`"));
        }
        let variant = adt.non_enum_variant();
        let expected = |kind: &str| Expr::str(container.expecting.clone().unwrap_or(format!("{kind} {type_name}")));
        if container.transparent {
            return self.read_transparent(json, adt, variant, args, span);
        }
        match variant.ctor_kind() {
            Some(CtorKind::Const) => Ok(method("unit", vec![expected("unit struct")])),
            // A newtype is what it holds (`visit_newtype_struct`).
            Some(CtorKind::Fn) if variant.fields.len() == 1 => {
                let field = variant.fields.iter().next().expect("a field");
                if self.serde_attrs(field.did)?.skip_deserializing {
                    return Err(self.unsupported(span, "a newtype struct whose field is skipped"));
                }
                let read = self.json_reader(field.ty(self.tcx, args), span)?;
                let value = Expr::call(read, vec![Expr::var(json)]);
                Ok(self.construct(adt, variant, args, vec![value]))
            }
            Some(CtorKind::Fn) => {
                let table = self.field_table(adt, variant, args, None, &container, span)?;
                Ok(method("tupleStruct", table.args(expected("tuple struct"))))
            }
            None => {
                let table = self.field_table(adt, variant, args, container.de_rename_all, &container, span)?;
                Ok(method("struct", table.args(expected("struct"))))
            }
        }
    }

    /// `#[serde(transparent)]`: the one field that's read, as it's read; the
    /// others are their defaults.
    fn read_transparent(
        &mut self,
        json: &str,
        adt: ty::AdtDef<'tcx>,
        variant: &ty::VariantDef,
        args: ty::GenericArgsRef<'tcx>,
        span: Span,
    ) -> R<Expr> {
        let mut items = Vec::new();
        for field in &variant.fields {
            let attrs = self.serde_attrs(field.did)?;
            let field_ty = field.ty(self.tcx, args);
            items.push(match attrs.default {
                // serde's `transparent` is the field without a default, which
                // a skipped one has.
                None if !attrs.skip_deserializing => {
                    let read = self.json_reader(field_ty, span)?;
                    Expr::call(read, vec![Expr::var(json)])
                }
                Some(SerdeDefault::Path) => self.default_path(field.did, span)?,
                // A `PhantomData`.
                None if self.is_std_adt(field_ty, Symbol::intern("PhantomData")) => Expr::undefined(),
                _ => self.default_value(field_ty, span)?,
            });
        }
        Ok(self.construct(adt, variant, args, items))
    }

    /// `#[serde(default = "path")]` on `def_id`: a call of the function.
    fn default_path(&mut self, def_id: DefId, span: Span) -> R<Expr> {
        match self.resolved_path(def_id, "default", false) {
            Some(f) if self.krate.fns.contains_key(&f) => Ok(Expr::call(self.fn_ref(f), Vec::new())),
            _ => Err(self.unsupported(span, "this `#[serde(default = ..)]`")),
        }
    }

    /// Each field that's read, with the name it's read by (for a struct, not
    /// a tuple), and what the value is made of.
    fn field_table(
        &mut self,
        adt: ty::AdtDef<'tcx>,
        variant: &ty::VariantDef,
        args: ty::GenericArgsRef<'tcx>,
        rule: Option<Rule>,
        container: &Attrs,
        span: Span,
    ) -> R<Table> {
        let named = variant.ctor_kind().is_none();
        let keys = self.field_keys(adt, variant, args);
        let defaults = container.default.map(|_| self.fresh("defaults"));
        let mut entries = Vec::new();
        let mut params = Vec::new();
        let mut items = Vec::new();
        for (i, field) in variant.fields.iter().enumerate() {
            let attrs = self.serde_attrs(field.did)?;
            let field_ty = field.ty(self.tcx, args);
            // A skipped field is `Default::default()`, unless the container
            // has a default.
            let default = match attrs.default {
                None if attrs.skip_deserializing && container.default.is_none() => Some(SerdeDefault::Default),
                default => default,
            };
            let missing = match (default, &defaults) {
                (Some(SerdeDefault::Default), _) => Some(self.default_value(field_ty, span)?),
                (Some(SerdeDefault::Path), _) => Some(self.default_path(field.did, span)?),
                (None, Some(defaults)) => Some(Expr::member(Expr::var(defaults), keys[i].clone())),
                (None, None) => None,
            };
            if attrs.skip_deserializing {
                items.push(missing.expect("a skipped field has a default"));
                continue;
            }
            let read = self.json_reader(field_ty, span)?;
            let missing = missing.map(|value| {
                let params = defaults.iter().map(|d| Pattern::from(d.as_str())).collect();
                Expr::arrow(params, vec![StmtKind::Return(Some(value)).at(js::Span::NONE)])
            });
            let mut entry = Vec::new();
            if named {
                let rust_name = field.name.to_string();
                let rust_name = rust_name.strip_prefix("r#").unwrap_or(&rust_name).to_string();
                let name = attrs
                    .de_rename
                    .clone()
                    .unwrap_or_else(|| rule.map_or(rust_name.clone(), |r| r.field(&rust_name)));
                entry.push(names_entry(name, &attrs.aliases));
            }
            entry.push(read);
            entry.extend(missing);
            entries.push(match <[Expr; 1]>::try_from(entry) {
                // A tuple's field that's read as it is.
                Ok([read]) => read,
                Err(entry) => Expr::array(entry),
            });
            let param = self.fresh(&keys[i]);
            items.push(Expr::var(&param));
            params.push(param);
        }
        // Built as it's read: a tuple struct's values are the array of them.
        let as_read = !named && adt.is_struct() && items.len() == params.len() && defaults.is_none();
        let build = (!as_read).then(|| {
            let value = self.construct(adt, variant, args, items);
            let mut patterns = vec![Pattern::Array(params.into_iter().map(Some).collect())];
            patterns.extend(defaults.iter().map(|d| Pattern::from(d.as_str())));
            Expr::arrow(patterns, vec![StmtKind::Return(Some(value)).at(js::Span::NONE)])
        });
        let mut options = Vec::new();
        if container.deny_unknown_fields && named {
            options.push(Prop::Field("deny".into(), Expr::bool(true)));
        }
        if let Some(default) = container.default
            && adt.is_struct()
        {
            let self_ty = Ty::new_adt(self.tcx, adt, args);
            let value = match default {
                SerdeDefault::Default => self.default_value(self_ty, span)?,
                SerdeDefault::Path => self.default_path(adt.did(), span)?,
            };
            let thunk = Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(value)).at(js::Span::NONE)]);
            options.push(Prop::Field("container".into(), thunk));
        }
        if let Some(expecting) = &container.expecting {
            options.push(Prop::Field("expecting".into(), Expr::str(expecting.as_str())));
        }
        Ok(Table {
            entries,
            build,
            options,
        })
    }

    /// Each field's key in JS: its name, or `_0` of a variant.
    fn field_keys(
        &self,
        adt: ty::AdtDef<'tcx>,
        variant: &ty::VariantDef,
        args: ty::GenericArgsRef<'tcx>,
    ) -> Vec<String> {
        if adt.is_enum() {
            return self
                .variant_fields(variant, args)
                .into_iter()
                .map(|(key, _)| key)
                .collect();
        }
        match self.shape(Ty::new_adt(self.tcx, adt, args)) {
            Shape::Object(fields) => fields.into_iter().map(|(key, _)| key).collect(),
            _ => (0..variant.fields.len()).map(|i| format!("_{i}")).collect(),
        }
    }

    /// A value of `variant`, of these fields, as rust-js makes one (ADRs
    /// 0013, 0020 and 0033).
    fn construct(
        &self,
        adt: ty::AdtDef<'tcx>,
        variant: &ty::VariantDef,
        args: ty::GenericArgsRef<'tcx>,
        items: Vec<Expr>,
    ) -> Expr {
        if adt.is_enum() {
            let name = variant_name(self.tcx, variant);
            if variant.fields.is_empty() {
                return Expr::str(name);
            }
            let fields = self.field_keys(adt, variant, args).into_iter().zip(items);
            let tag = Prop::Field("TAG".into(), Expr::str(name));
            return Expr::object(
                std::iter::once(tag)
                    .chain(fields.map(|(k, v)| Prop::Field(k, v)))
                    .collect(),
            );
        }
        if variant.ctor_kind() == Some(CtorKind::Const) {
            return Expr::undefined();
        }
        match self.shape(Ty::new_adt(self.tcx, adt, args)) {
            Shape::Object(fields) => Expr::object(
                fields
                    .into_iter()
                    .zip(items)
                    .map(|((key, _), value)| Prop::Field(key, value))
                    .collect(),
            ),
            Shape::Array(_) => Expr::array(items),
            Shape::Other => Expr::undefined(),
        }
    }

    /// An externally tagged enum: `json.enum(names, (variant, content) => ..)`,
    /// each variant read by its name, as its style reads it.
    fn read_enum(
        &mut self,
        json: &str,
        adt: ty::AdtDef<'tcx>,
        args: ty::GenericArgsRef<'tcx>,
        container: &Attrs,
        type_name: &str,
        span: Span,
    ) -> R<Expr> {
        let tagging = match (&container.tag, &container.content, container.untagged) {
            (_, _, true) => Some("an untagged enum"),
            (Some(_), Some(_), _) => Some("an adjacently tagged enum"),
            (Some(_), None, _) => Some("an internally tagged enum"),
            _ => None,
        };
        if let Some(tagging) = tagging {
            return Err(self.unsupported(span, &format!("deserializing {tagging}")));
        }
        let (variant_var, content) = (self.fresh("variant"), self.fresh("content"));
        let mut names = Vec::new();
        let mut other = None;
        let mut branches = Vec::new();
        for variant in adt.variants() {
            let attrs = self.serde_attrs(variant.def_id)?;
            if attrs.untagged {
                return Err(self.unsupported(span, "deserializing an `#[serde(untagged)]` variant"));
            }
            if attrs.skip_deserializing {
                continue;
            }
            let rust_name = variant.name.to_string();
            let name = attrs.de_rename.clone().unwrap_or_else(|| {
                container
                    .de_rename_all
                    .map_or(rust_name.clone(), |r| r.variant(&rust_name))
            });
            names.push(names_entry(name.clone(), &attrs.aliases));
            if attrs.other {
                other = Some(name.clone());
            }
            let rule = attrs.de_rename_all.or(container.de_rename_all_fields);
            let body = self.read_variant(&content, adt, variant, args, rule, container, type_name, span)?;
            branches.push((name, body));
        }
        // `if (variant === "Dot") { .. } else if ..`, the last one without a test.
        let mut chain: Option<Vec<Stmt>> = None;
        for (name, body) in branches.into_iter().rev() {
            chain = Some(match chain {
                None => body,
                Some(rest) => {
                    let test = Expr::bin(Op::Eq, Expr::var(&variant_var), Expr::str(name));
                    vec![StmtKind::If(test, body, Some(rest)).at(js::Span::NONE)]
                }
            });
        }
        let visit = Expr::arrow(vec![variant_var.into(), content.into()], chain.unwrap_or_default());
        let mut call_args = vec![Expr::array(names), visit];
        call_args.extend(other.map(Expr::str));
        Ok(Expr::call(Expr::member(Expr::var(json), "enum"), call_args))
    }

    /// One variant, from `content`, what it holds: nothing, one value, a
    /// tuple's or a struct's.
    #[allow(clippy::too_many_arguments)]
    fn read_variant(
        &mut self,
        content: &str,
        adt: ty::AdtDef<'tcx>,
        variant: &ty::VariantDef,
        args: ty::GenericArgsRef<'tcx>,
        rule: Option<Rule>,
        container: &Attrs,
        type_name: &str,
        span: Span,
    ) -> R<Vec<Stmt>> {
        let method = |name: &str, args: Vec<Expr>| Expr::call(Expr::member(Expr::var(content), name), args);
        let ret = |value: Expr| StmtKind::Return(Some(value)).at(js::Span::NONE);
        let expected = |kind: &str| {
            Expr::str(
                container
                    .expecting
                    .clone()
                    .unwrap_or(format!("{kind} {type_name}::{}", variant.name)),
            )
        };
        let unit = StmtKind::Expr(method("unit", Vec::new())).at(js::Span::NONE);
        Ok(match variant.ctor_kind() {
            Some(CtorKind::Const) => vec![unit, ret(self.construct(adt, variant, args, Vec::new()))],
            Some(CtorKind::Fn) if variant.fields.len() == 1 => {
                let field = variant.fields.iter().next().expect("a field");
                let attrs = self.serde_attrs(field.did)?;
                let field_ty = field.ty(self.tcx, args);
                // A newtype whose field is skipped holds nothing, and its
                // field is its default.
                if attrs.skip_deserializing {
                    let value = match attrs.default {
                        Some(SerdeDefault::Path) => self.default_path(field.did, span)?,
                        _ => self.default_value(field_ty, span)?,
                    };
                    vec![unit, ret(self.construct(adt, variant, args, vec![value]))]
                } else {
                    let read = self.json_reader(field_ty, span)?;
                    let value = method("newtype", vec![read]);
                    vec![ret(self.construct(adt, variant, args, vec![value]))]
                }
            }
            Some(CtorKind::Fn) => {
                let table = self.field_table(adt, variant, args, None, container, span)?;
                vec![ret(method("tuple", table.args(expected("tuple variant"))))]
            }
            None => {
                let table = self.field_table(adt, variant, args, rule, container, span)?;
                vec![ret(method("struct", table.args(expected("struct variant"))))]
            }
        })
    }
}

impl Table {
    /// `(expected, [..], build, { .. })`, leaving out what's not needed.
    fn args(self, expected: Expr) -> Vec<Expr> {
        let mut args = vec![expected, Expr::array(self.entries)];
        match (self.build, self.options.is_empty()) {
            (Some(build), true) => args.push(build),
            (build, false) => {
                args.push(build.unwrap_or_else(Expr::undefined));
                args.push(Expr::object(self.options));
            }
            (None, true) => {}
        }
        args
    }
}

/// A field's or a variant's name, `"name"`, or with its aliases,
/// `["name", "alias"]`, which match it too.
fn names_entry(name: String, aliases: &[String]) -> Expr {
    let aliases: Vec<&String> = aliases.iter().filter(|a| **a != name).collect();
    if aliases.is_empty() {
        return Expr::str(name);
    }
    let mut names = vec![Expr::str(name.as_str())];
    let mut seen = Vec::new();
    for alias in aliases {
        if !seen.contains(&alias) {
            names.push(Expr::str(alias.as_str()));
            seen.push(alias);
        }
    }
    Expr::array(names)
}
