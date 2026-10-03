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
//!
//! A tagged or untagged enum is read as serde reads one, through the value
//! read first and kept (ADR 0079).

use super::{Attrs, Rule, SerdeDefault, Tagging};
use crate::js::{self, Expr, Op, Pattern, Prop, Stmt, StmtKind};
use crate::lower::bindings::variant_name;
use crate::lower::representation::Num;
use crate::lower::{FnCx, R, Shape};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_hir::def::CtorKind;
use rustc_middle::ty::{self, Ty};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, sym};

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
        self.codec_fn(ty, false)
    }

    fn use_reader(&mut self) {
        self.runtime.insert(Helper::FromJson);
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
        let self_ty = self.tcx.type_of(imp).instantiate_identity().skip_normalization();
        let ty::Adt(adt, args) = *self_ty.kind() else {
            return Err(self.unsupported(span, "deserializing this"));
        };
        self.use_reader();
        let json = self.fresh("json");
        let mut params = vec![json.clone().into()];
        params.extend(self.codec_params(args, "read"));
        let value = self.read_adt(&json, adt, args, span)?;
        let js_span = self.js_span(span);
        Ok(js::Function {
            name: self.krate.fns[&method].name.clone(),
            params,
            body: vec![StmtKind::Return(Some(value)).at(js_span)],
            export: false,
            is_async: false,
            span: js_span,
            name_span: js_span,
        })
    }

    /// The function that reads a `ty` as serde's impl for it does:
    /// `$json.u32`, `$json.vec($json.string)`, `orderDeserialize_deserialize`.
    pub(in crate::lower) fn json_reader(&mut self, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let reader = |name: &str| Expr::member(Expr::var("$json"), name);
        let unsupported = |this: &Self| this.unsupported(span, &format!("deserializing `{ty}`"));
        if let Some(read) = self.serde_evidence(ty, false) {
            return Ok(read);
        }
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
        // serde_json's `Value` and `Number` (ADR 0083); its `Map` is a map.
        if self.json_type(ty) == Some(super::Json::Value) {
            self.runtime.insert(Helper::JsonValue);
            return Ok(reader("value"));
        }
        if self.is_json_number(ty) {
            return Ok(reader("number"));
        }
        // A `&str` borrows from the text.
        if let ty::Ref(_, inner, _) = ty.kind()
            && inner.is_str()
        {
            return Ok(reader("str"));
        }
        if let Some(inner) = self.option_of(ty) {
            let read = self.json_reader(inner, span)?;
            // A `Some` that looks like `None` is boxed (ADR 0051).
            let read = if self.boxed_payload(inner) {
                self.runtime.insert(Helper::Some);
                Expr::call(reader("some"), vec![read])
            } else {
                read
            };
            return Ok(Expr::call(reader("option"), vec![read]));
        }
        match ty.kind() {
            ty::Adt(_, args) if ty.is_box() || self.is_rc(ty) => self.json_reader(args.type_at(0), span),
            ty::Adt(_, args) if self.is_std_adt(ty, Symbol::intern("Result")) => {
                let (ok, err) = (
                    self.json_reader(args.type_at(0), span)?,
                    self.json_reader(args.type_at(1), span)?,
                );
                Ok(Expr::call(reader("result"), vec![ok, err]))
            }
            // A heap would have to be put in its order.
            ty::Adt(..) if self.is_std_adt(ty, Symbol::intern("BinaryHeap")) => Err(unsupported(self)),
            ty::Adt(_, args) if self.is_vec_like(ty) => {
                let read = self.json_reader(args.type_at(0), span)?;
                Ok(Expr::call(reader("vec"), vec![read]))
            }
            ty::Adt(_, args) if self.is_set(ty) => {
                let item = args.type_at(0);
                let read = self.json_reader(item, span)?;
                // Of items found by their value, a `$KeySet` of the array's
                // (ADR 0121), as JSON's `set` makes a `Set`.
                if self.is_value_key(item) && !self.is_js_key(item) {
                    let class = self.map_class(true, Some(item));
                    let items = Expr::call(Expr::call(reader("vec"), vec![read]), vec![Expr::var("json")]);
                    let made = Expr::new_(class, vec![items]);
                    return Ok(Expr::arrow(
                        vec!["json".into()],
                        vec![StmtKind::Return(Some(made)).at(js::Span::NONE)],
                    ));
                }
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
            // A generic one's, with the readers of its type's arguments.
            ty::Adt(_, args) if let Some(deserialize) = self.deserialize_fn(ty) => {
                let callee = self.fn_ref(deserialize);
                let readers = args.types().map(|t| self.json_reader(t, span)).collect::<R<Vec<_>>>()?;
                if readers.is_empty() {
                    return Ok(callee);
                }
                Ok(Expr::call(
                    reader("with"),
                    std::iter::once(callee).chain(readers).collect(),
                ))
            }
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
        // `#[serde(from = "T")]` and `try_from`: a `T`, read, then converted.
        if container.from || container.try_from {
            let convert = if container.from { sym::From } else { sym::TryFrom };
            let Some(source) = self.conversion(adt.did(), convert, false) else {
                return Err(self.unsupported(span, "this `#[serde(from)]`"));
            };
            let this = Ty::new_adt(self.tcx, adt, args);
            let read = self.json_reader(source, span)?;
            let value = self.convert(convert, this, source, Expr::call(read, vec![Expr::var(json)]), span)?;
            if container.from {
                return Ok(value);
            }
            // An `Err` is serde's `Error::custom` of it: its `Display`.
            let error = self.try_from_error(this, source, span)?;
            let e = self.fresh("error");
            let shown = self.display_string(Expr::var(&e), error, span)?;
            let display = Expr::arrow(vec![e.into()], vec![StmtKind::Return(Some(shown)).at(js::Span::NONE)]);
            return Ok(Expr::call(
                Expr::member(Expr::var("$json"), "tried"),
                vec![value, display],
            ));
        }
        if adt.is_enum() {
            return self.read_enum(json, adt, args, &container, &type_name, span);
        }
        let variant = adt.non_enum_variant();
        let expected = |kind: &str| Expr::str(container.expecting.clone().unwrap_or(format!("{kind} {type_name}")));
        if container.transparent {
            return self.read_transparent(json, adt, variant, args, span);
        }
        match variant.ctor_kind() {
            Some(CtorKind::Const) => Ok(method("unitStruct", vec![expected("unit struct")])),
            // A newtype is what it holds (`visit_newtype_struct`).
            Some(CtorKind::Fn) if variant.fields.len() == 1 => {
                let field = variant.fields.iter().next().expect("a field");
                if self.serde_attrs(field.did)?.skip_deserializing {
                    return Err(self.unsupported(span, "a newtype struct whose field is skipped"));
                }
                let read = self.json_reader(self.field_ty(field, args), span)?;
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

    /// `<to as TryFrom<from>>::Error`.
    fn try_from_error(&self, to: Ty<'tcx>, from: Ty<'tcx>, span: Span) -> R<Ty<'tcx>> {
        let trait_id = self.tcx.get_diagnostic_item(sym::TryFrom).expect("std has it");
        let error = self
            .tcx
            .associated_items(trait_id)
            .in_definition_order()
            .find(|item| item.is_type())
            .ok_or_else(|| self.unsupported(span, "this `#[serde(try_from)]`"))?;
        let projection = Ty::new_projection(self.tcx, ty::IsRigid::No, error.def_id, [to, from]);
        Ok(self
            .tcx
            .normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(projection)))
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
            let field_ty = self.field_ty(field, args);
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
        // `#[serde(flatten)]`: read after the others, from what they left.
        let (mut flattened, mut flat_params) = (Vec::new(), Vec::new());
        for (i, field) in variant.fields.iter().enumerate() {
            let attrs = self.serde_attrs(field.did)?;
            let field_ty = self.field_ty(field, args);
            if attrs.flatten && !attrs.skip_deserializing {
                flattened.push(self.json_reader(field_ty, span)?);
                let param = self.fresh(&keys[i]);
                items.push(Expr::var(&param));
                flat_params.push(param);
                continue;
            }
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
        let has_flatten = !flattened.is_empty();
        params.extend(flat_params);
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
        if has_flatten {
            options.push(Prop::Field("flatten".into(), Expr::array(flattened)));
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

    /// An enum, read as its tagging reads it (serde's "enum
    /// representations"), each variant by its name. Variants marked
    /// `#[serde(untagged)]`, which come last, are tried after the others.
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
            (_, _, true) => Tagging::Untagged,
            (Some(tag), Some(content), _) => Tagging::Adjacent(tag.clone(), content.clone()),
            (Some(tag), None, _) => Tagging::Internal(tag.clone()),
            _ => Tagging::External,
        };
        let mut tagged = Vec::new();
        let mut untagged = Vec::new();
        for variant in adt.variants() {
            let attrs = self.serde_attrs(variant.def_id)?;
            if attrs.skip_deserializing {
                continue;
            }
            if tagging == Tagging::Untagged || attrs.untagged {
                untagged.push((variant, attrs));
            } else {
                tagged.push((variant, attrs));
            }
        }
        if untagged.is_empty() {
            return self.read_tagged(json, &tagging, &tagged, adt, args, container, type_name, span);
        }
        // `json.untagged(message, [(content) => .., ..])`: the tagged
        // variants first, all at once, then each untagged one. Each is its own
        // function, so each can use the same names.
        let mut attempts = Vec::new();
        let names = self.names.clone();
        if !tagged.is_empty() {
            let reader = self.fresh("content");
            let value = self.read_tagged(&reader, &tagging, &tagged, adt, args, container, type_name, span)?;
            let body = vec![StmtKind::Return(Some(value)).at(js::Span::NONE)];
            attempts.push(Expr::arrow(vec![reader.into()], body));
            self.names = names.clone();
        }
        for (variant, attrs) in &untagged {
            let reader = self.fresh("content");
            let rule = attrs.de_rename_all.or(container.de_rename_all_fields);
            let body = self.read_variant(
                &reader,
                Form::Untagged,
                adt,
                variant,
                args,
                rule,
                container,
                type_name,
                span,
            )?;
            attempts.push(Expr::arrow(vec![reader.into()], body));
            self.names = names.clone();
        }
        let message = container
            .expecting
            .clone()
            .unwrap_or(format!("data did not match any variant of untagged enum {type_name}"));
        let call_args = vec![Expr::str(message), Expr::array(attempts)];
        Ok(Expr::call(Expr::member(Expr::var(json), "untagged"), call_args))
    }

    /// The tagged variants: `json.enum(names, (variant, content) => ..)`, or
    /// `json.internallyTagged(tag, ..)`, or `json.adjacentlyTagged(tag, content, ..)`.
    #[allow(clippy::too_many_arguments)]
    fn read_tagged(
        &mut self,
        json: &str,
        tagging: &Tagging,
        variants: &[(&ty::VariantDef, Attrs)],
        adt: ty::AdtDef<'tcx>,
        args: ty::GenericArgsRef<'tcx>,
        container: &Attrs,
        type_name: &str,
        span: Span,
    ) -> R<Expr> {
        let (variant_var, content) = (self.fresh("variant"), self.fresh("content"));
        let form = match tagging {
            Tagging::External => Form::External,
            Tagging::Internal(_) => Form::Internal,
            _ => Form::Untagged,
        };
        let mut names = Vec::new();
        let mut other = None;
        let mut branches = Vec::new();
        for (variant, attrs) in variants {
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
            // What one variant's reading names is its own.
            let names = self.names.clone();
            let body = self.read_variant(&content, form, adt, variant, args, rule, container, type_name, span)?;
            self.names = names;
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
        let expected = |kind: &str| Expr::str(container.expecting.clone().unwrap_or(format!("{kind} {type_name}")));
        let method = |name: &str, args: Vec<Expr>| Expr::call(Expr::member(Expr::var(json), name), args);
        Ok(match tagging {
            Tagging::Internal(tag) => {
                let mut call_args = vec![
                    Expr::str(tag.as_str()),
                    expected("internally tagged enum"),
                    Expr::array(names),
                    visit,
                ];
                call_args.extend(other.map(Expr::str));
                method("internallyTagged", call_args)
            }
            Tagging::Adjacent(tag, body) => {
                let mut options = Vec::new();
                if container.deny_unknown_fields {
                    options.push(Prop::Field("deny".into(), Expr::bool(true)));
                }
                if let Some(other) = other {
                    options.push(Prop::Field("other".into(), Expr::str(other)));
                }
                let mut call_args = vec![
                    Expr::str(tag.as_str()),
                    Expr::str(body.as_str()),
                    expected("adjacently tagged enum"),
                    Expr::array(names),
                    visit,
                ];
                if !options.is_empty() {
                    call_args.push(Expr::object(options));
                }
                method("adjacentlyTagged", call_args)
            }
            _ => {
                let mut call_args = vec![Expr::str(type_name), Expr::array(names), visit];
                call_args.extend(other.map(Expr::str));
                method("enum", call_args)
            }
        })
    }

    /// One variant, from `content`, what it holds: nothing, one value, a
    /// tuple's or a struct's, as the enum's tagging reads it.
    #[allow(clippy::too_many_arguments)]
    fn read_variant(
        &mut self,
        content: &str,
        form: Form,
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
        // A unit variant's check that it holds nothing.
        let unit = {
            let expected = Expr::str(format!("unit variant {type_name}::{}", variant.name));
            let check = match form {
                Form::External => method("unit", Vec::new()),
                Form::Internal => method("taggedUnit", vec![expected]),
                Form::Untagged => method("untaggedUnit", vec![expected]),
            };
            StmtKind::Expr(check).at(js::Span::NONE)
        };
        Ok(match variant.ctor_kind() {
            Some(CtorKind::Const) => vec![unit, ret(self.construct(adt, variant, args, Vec::new()))],
            Some(CtorKind::Fn) if variant.fields.len() == 1 => {
                let field = variant.fields.iter().next().expect("a field");
                let attrs = self.serde_attrs(field.did)?;
                let field_ty = self.field_ty(field, args);
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
                    let value = match form {
                        Form::External => method("newtype", vec![read]),
                        _ => Expr::call(read, vec![Expr::var(content)]),
                    };
                    vec![ret(self.construct(adt, variant, args, vec![value]))]
                }
            }
            Some(CtorKind::Fn) => {
                let table = self.field_table(adt, variant, args, None, container, span)?;
                let name = match form {
                    Form::External => "tuple",
                    Form::Untagged => "tupleStruct",
                    // serde_derive rejects it.
                    Form::Internal => return Err(self.unsupported(span, "an internally tagged tuple variant")),
                };
                vec![ret(method(name, table.args(expected("tuple variant"))))]
            }
            None => {
                let table = self.field_table(adt, variant, args, rule, container, span)?;
                let name = if form == Form::Untagged {
                    "untaggedStruct"
                } else {
                    "struct"
                };
                vec![ret(method(name, table.args(expected("struct variant"))))]
            }
        })
    }
}

/// How an enum's tagging reads a variant's content: serde's
/// `VariantAccess` of `{"Name": ..}` (external), what's left of an object
/// beside its tag (internal), or the value itself (untagged, and adjacent).
#[derive(Clone, Copy, PartialEq)]
enum Form {
    External,
    Internal,
    Untagged,
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
